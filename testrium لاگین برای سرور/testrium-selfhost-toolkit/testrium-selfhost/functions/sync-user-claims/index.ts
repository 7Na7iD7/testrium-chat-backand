import { serve } from "https://deno.land/std@0.224.0/http/server.ts";
import { createClient } from "https://esm.sh/@supabase/supabase-js@2";

const SUPABASE_URL = Deno.env.get("SUPABASE_URL")!;
const SERVICE_ROLE_KEY = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;
const SHARED_SECRET = Deno.env.get("SYNC_CLAIMS_SHARED_SECRET")!;
const CHAT_SERVER_URL = Deno.env.get("CHAT_SERVER_URL");
const CHAT_SERVER_NOTIFY_SECRET = Deno.env.get("CHAT_SERVER_NOTIFY_SECRET");

const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, content-type",
};

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
    function: "sync-user-claims",
    timestamp: new Date().toISOString(),
    ...data,
  }));
}

function timingSafeEqual(a: string, b: string): boolean {
  const encoder = new TextEncoder();
  const bytesA = encoder.encode(a);
  const bytesB = encoder.encode(b);
  const maxLen = Math.max(bytesA.length, bytesB.length);
  let diff = bytesA.length === bytesB.length ? 0 : 1;
  for (let i = 0; i < maxLen; i++) {
    diff |= (bytesA[i] ?? 0) ^ (bytesB[i] ?? 0);
  }
  return diff === 0;
}

type WebhookPayload = {
  type: "INSERT" | "UPDATE" | "DELETE";
  table: string;
  record: Record<string, unknown> | null;
  old_record: Record<string, unknown> | null;
};

async function notifyChatServer(role: "student" | "professor", identifier: string) {
  if (!CHAT_SERVER_URL || !CHAT_SERVER_NOTIFY_SECRET) {
    log("warn", "chat_server_notify_skipped_not_configured", { role, identifier });
    return;
  }

  try {
    const response = await fetch(`${CHAT_SERVER_URL}/internal/notify-claims-updated`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Authorization": `Bearer ${CHAT_SERVER_NOTIFY_SECRET}`,
      },
      body: JSON.stringify({ role, identifier }),
    });

    if (!response.ok) {
      log("warn", "chat_server_notify_failed", { role, identifier, status: response.status });
      return;
    }

    const body = await response.json().catch(() => null);
    log("info", "chat_server_notified", { role, identifier, delivered: body?.delivered ?? null });
  } catch (e) {
    log("warn", "chat_server_notify_error", { role, identifier, error: String(e) });
  }
}

async function syncStudent(admin: ReturnType<typeof createClient>, studentCode: string) {
  const { data: student, error: studentError } = await admin
    .from("students")
    .select("student_code, first_name, last_name, auth_user_id")
    .eq("student_code", studentCode)
    .maybeSingle();

  if (studentError || !student || !student.auth_user_id) {
    if (studentError) log("error", "student_lookup_failed", { db_error: studentError.message });
    return;
  }

  const { data: enrollments, error: enrollError } = await admin
    .from("enrollments")
    .select("section_id")
    .eq("student_code", student.student_code)
    .eq("status", "enrolled");

  if (enrollError) {
    log("error", "enrollments_lookup_failed", { db_error: enrollError.message });
    return;
  }

  const { error: claimsError } = await admin.auth.admin.updateUserById(student.auth_user_id, {
    app_metadata: {
      role: "student",
      user_code: student.student_code,
      display_name: `${student.first_name} ${student.last_name}`,
      accessible_section_ids: (enrollments ?? []).map((e) => e.section_id),
    },
  });

  if (claimsError) {
    log("error", "student_claims_update_failed", { student_code: studentCode, db_error: claimsError.message });
    return;
  }

  log("info", "student_claims_synced", { student_code: studentCode, section_count: enrollments?.length ?? 0 });
  await notifyChatServer("student", student.student_code);
}

async function syncProfessor(admin: ReturnType<typeof createClient>, professorId: string) {
  const { data: professor, error: professorError } = await admin
    .from("professors")
    .select("id, professor_code, full_name, auth_user_id")
    .eq("id", professorId)
    .maybeSingle();

  if (professorError || !professor || !professor.auth_user_id) {
    if (professorError) log("error", "professor_lookup_failed", { db_error: professorError.message });
    return;
  }

  const { data: sections, error: sectionsError } = await admin
    .from("sections")
    .select("section_id")
    .eq("professor_id", professor.id);

  if (sectionsError) {
    log("error", "sections_lookup_failed", { db_error: sectionsError.message });
    return;
  }

  const { error: claimsError } = await admin.auth.admin.updateUserById(professor.auth_user_id, {
    app_metadata: {
      role: "professor",
      user_code: professor.professor_code,
      display_name: professor.full_name,
      accessible_section_ids: (sections ?? []).map((s) => s.section_id),
    },
  });

  if (claimsError) {
    log("error", "professor_claims_update_failed", { professor_id: professorId, db_error: claimsError.message });
    return;
  }

  log("info", "professor_claims_synced", { professor_id: professorId, section_count: sections?.length ?? 0 });
  await notifyChatServer("professor", professor.id as string);
}

serve(async (req) => {
  if (req.method === "OPTIONS") return new Response("ok", { headers: corsHeaders });

  const authHeader = req.headers.get("Authorization")?.replace("Bearer ", "") ?? "";
  if (!authHeader || !timingSafeEqual(authHeader, SHARED_SECRET)) {
    log("warn", "unauthorized_caller");
    return json({ ok: false, message: "دسترسی غیرمجاز" }, 401);
  }

  let payload: WebhookPayload;
  try {
    payload = await req.json();
  } catch {
    log("warn", "invalid_body");
    return json({ ok: false, message: "بدنه‌ی درخواست نامعتبر است" }, 400);
  }

  const admin = createClient(SUPABASE_URL, SERVICE_ROLE_KEY);
  const row = payload.record ?? payload.old_record;

  if (payload.table === "enrollments" && row?.student_code) {
    await syncStudent(admin, row.student_code as string);
  } else if (payload.table === "sections" && row?.professor_id) {
    await syncProfessor(admin, row.professor_id as string);
  } else {
    log("warn", "unhandled_table", { table: payload.table });
  }

  return json({ ok: true });
});
