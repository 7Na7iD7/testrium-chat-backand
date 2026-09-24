import { serve } from "https://deno.land/std@0.224.0/http/server.ts";
import { createClient } from "https://esm.sh/@supabase/supabase-js@2";

const SUPABASE_URL = Deno.env.get("SUPABASE_URL")!;
const SERVICE_ROLE_KEY = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;
const CHAT_SERVER_SHARED_SECRET = Deno.env.get("CHAT_SERVER_SHARED_SECRET")!;

const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, x-client-info, apikey, content-type",
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
    function: "chat-identity",
    timestamp: new Date().toISOString(),
    ...data,
  }));
}

// مقایسه‌ی timing-safe — هم‌معادل timing_safe_eq سمت Rust
// (internal_handlers.rs). مقایسه‌ی معمولی `===` روی رشته زودتر از
// اولین کاراکتر متفاوت برمی‌گرده و تئوریاً از طریق تفاوت زمان پاسخ
// قابل حدس زدن است؛ این نسخه همیشه کل طول رو می‌خونه.
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

serve(async (req) => {
  if (req.method === "OPTIONS") return new Response("ok", { headers: corsHeaders });

  const sharedSecret = req.headers.get("Authorization")?.replace("Bearer ", "") ?? "";
  if (!sharedSecret || !timingSafeEqual(sharedSecret, CHAT_SERVER_SHARED_SECRET)) {
    log("warn", "unauthorized_caller");
    return json({ ok: false, message: "دسترسی غیرمجاز" }, 401);
  }

  const payload = await req.json().catch(() => null);
  const authUserId = (payload?.auth_user_id ?? "").trim();
  if (!authUserId) {
    log("warn", "missing_auth_user_id");
    return json({ ok: false, message: "auth_user_id ارسال نشده" }, 400);
  }

  const admin = createClient(SUPABASE_URL, SERVICE_ROLE_KEY);

  const { data: student, error: studentError } = await admin
    .from("students")
    .select("student_code, first_name, last_name, is_active")
    .eq("auth_user_id", authUserId)
    .maybeSingle();

  if (studentError) {
    log("error", "student_lookup_failed", { db_error: studentError.message });
  }

  if (student) {
    if (!student.is_active) {
      return json({ ok: false, message: "این کد غیرفعال است" }, 403);
    }

    const { data: enrollments, error: enrollError } = await admin
      .from("enrollments")
      .select("section_id")
      .eq("student_code", student.student_code)
      .eq("status", "enrolled");

    if (enrollError) {
      log("error", "enrollments_lookup_failed", { db_error: enrollError.message });
    }

    return json({
      ok: true,
      role: "student",
      identifier: student.student_code,
      display_name: `${student.first_name} ${student.last_name}`,
      accessible_section_ids: (enrollments ?? []).map((e) => e.section_id),
    });
  }

  const { data: professor, error: professorError } = await admin
    .from("professors")
    .select("id, full_name, is_active")
    .eq("auth_user_id", authUserId)
    .maybeSingle();

  if (professorError) {
    log("error", "professor_lookup_failed", { db_error: professorError.message });
  }

  if (professor) {
    if (!professor.is_active) {
      return json({ ok: false, message: "این کد غیرفعال است" }, 403);
    }

    const { data: sections, error: sectionsError } = await admin
      .from("sections")
      .select("section_id")
      .eq("professor_id", professor.id);

    if (sectionsError) {
      log("error", "sections_lookup_failed", { db_error: sectionsError.message });
    }

    return json({
      ok: true,
      role: "professor",
      identifier: professor.id,
      display_name: professor.full_name,
      accessible_section_ids: (sections ?? []).map((s) => s.section_id),
    });
  }

  log("warn", "identity_not_found", { auth_user_id: authUserId });
  return json({ ok: false, message: "کاربری با این شناسه یافت نشد" }, 404);
});
