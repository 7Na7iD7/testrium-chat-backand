import { serve } from "https://deno.land/std@0.224.0/http/server.ts";
import { createClient } from "https://esm.sh/@supabase/supabase-js@2";

const SUPABASE_URL = Deno.env.get("SUPABASE_URL")!;
const SERVICE_ROLE_KEY = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;

const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, x-client-info, apikey, content-type, x-admin-secret",
};

const RATE_LIMIT_WINDOW_MINUTES = 15;
const MAX_ATTEMPTS_PER_USER = 5;
const MAX_ATTEMPTS_PER_CODE = 8;

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { ...corsHeaders, "Content-Type": "application/json" },
  });
}

function log(level: "info" | "warn" | "error", event: string, data: Record<string, unknown> = {}) {
  console.log(JSON.stringify({
    level,
    event,
    function: "verify-code",
    timestamp: new Date().toISOString(),
    ...data,
  }));
}

async function countRecentAttempts(
  admin: ReturnType<typeof createClient>,
  column: "auth_user_id" | "code_attempted",
  value: string,
): Promise<number> {
  const since = new Date(Date.now() - RATE_LIMIT_WINDOW_MINUTES * 60 * 1000).toISOString();
  const { count, error } = await admin
    .from("verify_code_attempts")
    .select("id", { count: "exact", head: true })
    .eq(column, value)
    .eq("succeeded", false)
    .gte("attempted_at", since);

  if (error) {
    log("error", "rate_limit_check_failed", { column, db_error: error.message });
    return Number.MAX_SAFE_INTEGER;
  }
  return count ?? 0;
}

async function recordAttempt(
  admin: ReturnType<typeof createClient>,
  authUserId: string,
  code: string,
  succeeded: boolean,
) {
  const { error } = await admin.from("verify_code_attempts").insert({
    auth_user_id: authUserId,
    code_attempted: code,
    succeeded,
  });
  if (error) {
    log("error", "attempt_record_failed", { db_error: error.message });
  }
}

serve(async (req) => {
  const startedAt = Date.now();

  if (req.method === "OPTIONS") return new Response("ok", { headers: corsHeaders });

  let payload: { code?: string; auth_user_id?: string };
  try {
    payload = await req.json();
  } catch {
    log("warn", "invalid_body");
    return json({ ok: false, error_code: "bad_request", message: "بدنه‌ی درخواست نامعتبر است" }, 400);
  }

  const code = (payload.code ?? "").trim();
  const authUserId = (payload.auth_user_id ?? "").trim();

  if (!code || !authUserId) {
    log("warn", "missing_params", { code_present: !!code, auth_user_id_present: !!authUserId });
    return json({ ok: false, error_code: "bad_request", message: "کد یا شناسه‌ی کاربر ارسال نشده" }, 400);
  }

  const admin = createClient(SUPABASE_URL, SERVICE_ROLE_KEY);

  const [userAttempts, codeAttempts] = await Promise.all([
    countRecentAttempts(admin, "auth_user_id", authUserId),
    countRecentAttempts(admin, "code_attempted", code),
  ]);

  if (userAttempts >= MAX_ATTEMPTS_PER_USER) {
    log("warn", "rate_limited_by_user", { auth_user_id: authUserId, attempts: userAttempts });
    return json({
      ok: false,
      error_code: "rate_limited",
      message: `تعداد تلاش‌های ناموفق بیش از حد مجاز است. لطفاً ${RATE_LIMIT_WINDOW_MINUTES} دقیقه‌ی دیگر دوباره امتحان کنید.`,
    }, 429);
  }

  if (codeAttempts >= MAX_ATTEMPTS_PER_CODE) {
    log("warn", "rate_limited_by_code", { code_attempted: code, attempts: codeAttempts });
    return json({
      ok: false,
      error_code: "rate_limited",
      message: `این کد به دلیل تلاش‌های ناموفق زیاد، موقتاً قفل شده است. لطفاً ${RATE_LIMIT_WINDOW_MINUTES} دقیقه‌ی دیگر دوباره امتحان کنید.`,
    }, 429);
  }

  const { data: student, error: studentError } = await admin
    .from("students")
    .select("student_code, first_name, last_name, student_class, is_active, auth_user_id, relink_allowed_until")
    .eq("student_code", code)
    .maybeSingle();

  if (studentError) {
    log("error", "student_lookup_failed", { db_error: studentError.message });
  }

  if (student) {
    if (!student.is_active) {
      await recordAttempt(admin, authUserId, code, false);
      log("warn", "inactive_code", { role: "student", student_code: code });
      return json({ ok: false, error_code: "inactive", message: "این کد غیرفعال است" }, 403);
    }

    const isSameDevice = student.auth_user_id === authUserId;

    if (!isSameDevice) {
      log("info", "device_relinked", {
        role: "student",
        student_code: code,
        previous_auth_user_id: student.auth_user_id,
        new_auth_user_id: authUserId,
      });

      const { error: updateError } = await admin
        .from("students")
        .update({ auth_user_id: authUserId, relink_allowed_until: null })
        .eq("student_code", code);

      if (updateError) {
        log("error", "student_link_failed", { student_code: code, db_error: updateError.message });
      }
    }

    const { data: enrollments, error: enrollError } = await admin
      .from("enrollments")
      .select("section_id")
      .eq("student_code", student.student_code)
      .eq("status", "enrolled");

    if (enrollError) {
      log("error", "enrollments_lookup_failed", { db_error: enrollError.message });
    }

    const accessibleSectionIds = (enrollments ?? []).map((e) => e.section_id);

    const { error: claimsError } = await admin.auth.admin.updateUserById(authUserId, {
      app_metadata: {
        role: "student",
        user_code: student.student_code,
        display_name: `${student.first_name} ${student.last_name}`,
        accessible_section_ids: accessibleSectionIds,
      },
    });

    if (claimsError) {
      log("error", "student_claims_update_failed", { student_code: code, db_error: claimsError.message });
    }

    await recordAttempt(admin, authUserId, code, true);

    log("info", "verify_success", {
      role: "student",
      student_code: code,
      section_count: accessibleSectionIds.length,
      duration_ms: Date.now() - startedAt,
    });

    return json({
      ok: true,
      role: "student",
      profile: {
        student_id: authUserId,
        student_code: student.student_code,
        first_name: student.first_name,
        last_name: student.last_name,
        student_class: student.student_class,
      },
    });
  }

  const { data: professor, error: professorError } = await admin
    .from("professors")
    .select("id, professor_code, full_name, is_active, auth_user_id, relink_allowed_until")
    .eq("professor_code", code)
    .maybeSingle();

  if (professorError) {
    log("error", "professor_lookup_failed", { db_error: professorError.message });
  }

  if (professor) {
    if (!professor.is_active) {
      await recordAttempt(admin, authUserId, code, false);
      log("warn", "inactive_code", { role: "professor", professor_code: code });
      return json({ ok: false, error_code: "inactive", message: "این کد غیرفعال است" }, 403);
    }

    const isSameDevice = professor.auth_user_id === authUserId;

    if (!isSameDevice) {
      log("info", "device_relinked", {
        role: "professor",
        professor_code: code,
        previous_auth_user_id: professor.auth_user_id,
        new_auth_user_id: authUserId,
      });

      const { error: updateError } = await admin
        .from("professors")
        .update({ auth_user_id: authUserId, relink_allowed_until: null })
        .eq("id", professor.id);

      if (updateError) {
        log("error", "professor_link_failed", { professor_code: code, db_error: updateError.message });
      }
    }

    const { data: sections, error: sectionsError } = await admin
      .from("sections")
      .select("section_id")
      .eq("professor_id", professor.id);

    if (sectionsError) {
      log("error", "sections_lookup_failed", { db_error: sectionsError.message });
    }

    const accessibleSectionIds = (sections ?? []).map((s) => s.section_id);

    const { error: claimsError } = await admin.auth.admin.updateUserById(authUserId, {
      app_metadata: {
        role: "professor",
        user_code: professor.professor_code,
        display_name: professor.full_name,
        accessible_section_ids: accessibleSectionIds,
      },
    });

    if (claimsError) {
      log("error", "professor_claims_update_failed", { professor_code: code, db_error: claimsError.message });
    }

    await recordAttempt(admin, authUserId, code, true);

    log("info", "verify_success", {
      role: "professor",
      professor_code: code,
      section_count: accessibleSectionIds.length,
      duration_ms: Date.now() - startedAt,
    });

    return json({
      ok: true,
      role: "professor",
      profile: {
        professor_id: professor.id,
        professor_code: professor.professor_code,
        full_name: professor.full_name,
      },
    });
  }

  await recordAttempt(admin, authUserId, code, false);
  log("warn", "code_not_found", { duration_ms: Date.now() - startedAt });
  return json({ ok: false, error_code: "not_found", message: "کد وارد‌شده معتبر نیست" }, 404);
});
