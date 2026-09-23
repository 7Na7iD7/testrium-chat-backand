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
    function: "chat-verify-link",
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

serve(async (req) => {
  if (req.method === "OPTIONS") return new Response("ok", { headers: corsHeaders });

  const sharedSecret = req.headers.get("Authorization")?.replace("Bearer ", "") ?? "";
  if (!sharedSecret || !timingSafeEqual(sharedSecret, CHAT_SERVER_SHARED_SECRET)) {
    log("warn", "unauthorized_caller");
    return json({ linked: false }, 401);
  }

  const payload = await req.json().catch(() => null);
  const professorId = (payload?.professor_id ?? "").trim();
  const studentCode = (payload?.student_code ?? "").trim();

  if (!professorId || !studentCode) {
    log("warn", "missing_params", { professor_id: professorId, student_code: studentCode });
    return json({ linked: false }, 400);
  }

  const admin = createClient(SUPABASE_URL, SERVICE_ROLE_KEY);

  const { data, error } = await admin
    .from("enrollments")
    .select("section_id, sections!inner(professor_id)")
    .eq("student_code", studentCode)
    .eq("status", "enrolled")
    .eq("sections.professor_id", professorId)
    .limit(1);

  if (error) {
    log("error", "link_check_failed", { db_error: error.message, professor_id: professorId, student_code: studentCode });
    return json({ linked: false }, 500);
  }

  const linked = (data?.length ?? 0) > 0;
  log("info", "link_check_result", { professor_id: professorId, student_code: studentCode, linked });

  return json({ linked });
});
