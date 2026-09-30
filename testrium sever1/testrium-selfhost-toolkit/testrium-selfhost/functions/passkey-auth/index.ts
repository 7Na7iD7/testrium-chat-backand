import { serve } from "https://deno.land/std@0.224.0/http/server.ts";
import { createClient } from "https://esm.sh/@supabase/supabase-js@2";

const SUPABASE_URL = Deno.env.get("SUPABASE_URL")!;
const SERVICE_ROLE_KEY = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;

const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, x-client-info, apikey, content-type",
};

const CHALLENGE_TTL_SECONDS = 120;
const RATE_WINDOW_MINUTES = 15;
const MAX_CHALLENGES_PER_WINDOW = 20;
const MAX_ACTIVE_PASSKEYS = 5;

type Role = "student" | "professor";
type Admin = ReturnType<typeof createClient>;

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { ...corsHeaders, "Content-Type": "application/json" },
  });
}

function fail(errorCode: string, status: number) {
  return json({ ok: false, error_code: errorCode }, status);
}

function log(level: "info" | "warn" | "error", event: string, data: Record<string, unknown> = {}) {
  console.log(JSON.stringify({ level, event, function: "passkey-auth", timestamp: new Date().toISOString(), ...data }));
}

function b64ToBytes(s: string): Uint8Array {
  const bin = atob(s);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

function bytesToB64(b: Uint8Array): string {
  let s = "";
  for (const x of b) s += String.fromCharCode(x);
  return btoa(s);
}

function randomChallenge(): string {
  const b = new Uint8Array(32);
  crypto.getRandomValues(b);
  return bytesToB64(b);
}

function signedMessage(purpose: string, challengeId: string, challenge: string): Uint8Array {
  return new TextEncoder().encode("testrium-passkey-v1|" + purpose + "|" + challengeId + "|" + challenge);
}

async function verifySignature(publicKeyB64: string, signatureB64: string, message: Uint8Array): Promise<boolean> {
  try {
    const pub = b64ToBytes(publicKeyB64);
    const sig = b64ToBytes(signatureB64);
    if (pub.length !== 65 || pub[0] !== 4 || sig.length !== 64) return false;
    const key = await crypto.subtle.importKey("raw", pub, { name: "ECDSA", namedCurve: "P-256" }, false, ["verify"]);
    return await crypto.subtle.verify({ name: "ECDSA", hash: "SHA-256" }, key, sig, message);
  } catch (_) {
    return false;
  }
}

async function callerUserId(admin: Admin, req: Request): Promise<string | null> {
  const auth = req.headers.get("authorization") ?? "";
  const token = auth.startsWith("Bearer ") ? auth.slice(7) : "";
  if (!token) return null;
  const { data, error } = await admin.auth.getUser(token);
  if (error || !data.user) return null;
  return data.user.id;
}

interface LinkedIdentity {
  role: Role;
  code: string;
}

async function linkedIdentity(admin: Admin, authUserId: string): Promise<LinkedIdentity | null> {
  const student = await admin.from("students").select("student_code").eq("auth_user_id", authUserId).maybeSingle();
  if (student.data) return { role: "student", code: student.data.student_code as string };
  const professor = await admin.from("professors").select("professor_code").eq("auth_user_id", authUserId).maybeSingle();
  if (professor.data) return { role: "professor", code: professor.data.professor_code as string };
  return null;
}

async function createChallenge(admin: Admin, purpose: string, authUserId: string, passkeyId: string | null) {
  const challenge = randomChallenge();
  const expiresAt = new Date(Date.now() + CHALLENGE_TTL_SECONDS * 1000).toISOString();
  const { data, error } = await admin
    .from("passkey_challenges")
    .insert({ challenge, purpose, auth_user_id: authUserId, passkey_id: passkeyId, expires_at: expiresAt })
    .select("id")
    .single();
  if (error) throw error;
  return { challengeId: data.id as string, challenge };
}

async function consumeChallenge(admin: Admin, challengeId: string, purpose: string, authUserId: string) {
  const nowIso = new Date().toISOString();
  const { data, error } = await admin
    .from("passkey_challenges")
    .update({ used_at: nowIso })
    .eq("id", challengeId)
    .eq("purpose", purpose)
    .eq("auth_user_id", authUserId)
    .is("used_at", null)
    .gt("expires_at", nowIso)
    .select("challenge, passkey_id")
    .maybeSingle();
  if (error) throw error;
  if (!data) return null;
  return { challenge: data.challenge as string, passkeyId: data.passkey_id as string | null };
}

async function recentChallengeCount(admin: Admin, column: "passkey_id" | "auth_user_id", value: string): Promise<number> {
  const since = new Date(Date.now() - RATE_WINDOW_MINUTES * 60 * 1000).toISOString();
  const { count, error } = await admin
    .from("passkey_challenges")
    .select("id", { count: "exact", head: true })
    .eq(column, value)
    .gte("created_at", since);
  if (error) throw error;
  return count ?? 0;
}

async function linkAndIssueClaims(admin: Admin, authUserId: string, role: Role, code: string) {
  if (role === "student") {
    const { data: student } = await admin
      .from("students")
      .select("student_code, first_name, last_name, student_class, is_active, auth_user_id")
      .eq("student_code", code)
      .maybeSingle();
    if (!student) return { error: "invalid_passkey", status: 404 };
    if (!student.is_active) return { error: "inactive", status: 403 };

    if (student.auth_user_id !== authUserId) {
      await admin.from("students").update({ auth_user_id: authUserId, relink_allowed_until: null }).eq("student_code", code);
    }

    const { data: enrollments } = await admin
      .from("enrollments")
      .select("section_id")
      .eq("student_code", student.student_code)
      .eq("status", "enrolled");
    const sectionIds = (enrollments ?? []).map((e: any) => e.section_id);

    const displayName = student.first_name + " " + student.last_name;
    const { error: claimsError } = await admin.auth.admin.updateUserById(authUserId, {
      app_metadata: {
        role: "student",
        user_code: student.student_code,
        display_name: displayName,
        accessible_section_ids: sectionIds,
      },
    });
    if (claimsError) log("error", "student_claims_update_failed", { student_code: code, db_error: claimsError.message });

    return {
      role: "student",
      profile: {
        student_id: authUserId,
        student_code: student.student_code,
        first_name: student.first_name,
        last_name: student.last_name,
        student_class: student.student_class,
      },
    };
  }

  const { data: professor } = await admin
    .from("professors")
    .select("id, professor_code, full_name, is_active, auth_user_id")
    .eq("professor_code", code)
    .maybeSingle();
  if (!professor) return { error: "invalid_passkey", status: 404 };
  if (!professor.is_active) return { error: "inactive", status: 403 };

  if (professor.auth_user_id !== authUserId) {
    await admin.from("professors").update({ auth_user_id: authUserId, relink_allowed_until: null }).eq("id", professor.id);
  }

  const { data: sections } = await admin.from("sections").select("section_id").eq("professor_id", professor.id);
  const sectionIds = (sections ?? []).map((s: any) => s.section_id);

  const { error: claimsError } = await admin.auth.admin.updateUserById(authUserId, {
    app_metadata: {
      role: "professor",
      user_code: professor.professor_code,
      display_name: professor.full_name,
      accessible_section_ids: sectionIds,
    },
  });
  if (claimsError) log("error", "professor_claims_update_failed", { professor_code: code, db_error: claimsError.message });

  return {
    role: "professor",
    profile: {
      professor_id: professor.id,
      professor_code: professor.professor_code,
      full_name: professor.full_name,
    },
  };
}

serve(async (req) => {
  if (req.method === "OPTIONS") return new Response("ok", { headers: corsHeaders });

  const body = await req.json().catch(() => null);
  if (!body || typeof body.action !== "string") return fail("bad_request", 400);

  const admin = createClient(SUPABASE_URL, SERVICE_ROLE_KEY);
  const userId = await callerUserId(admin, req);
  if (!userId) return fail("unauthorized", 401);

  try {
    switch (body.action) {
      case "register_challenge": {
        const me = await linkedIdentity(admin, userId);
        if (!me) return fail("not_linked", 403);

        const { count } = await admin
          .from("user_passkeys")
          .select("id", { count: "exact", head: true })
          .eq("role", me.role)
          .eq("user_code", me.code)
          .is("revoked_at", null);
        if ((count ?? 0) >= MAX_ACTIVE_PASSKEYS) return fail("limit_reached", 409);

        if ((await recentChallengeCount(admin, "auth_user_id", userId)) >= MAX_CHALLENGES_PER_WINDOW) {
          return fail("rate_limited", 429);
        }

        const created = await createChallenge(admin, "register", userId, null);
        return json({ ok: true, challenge_id: created.challengeId, challenge: created.challenge });
      }

      case "register": {
        const { challenge_id, public_key, signature, device_label } = body as {
          challenge_id?: string; public_key?: string; signature?: string; device_label?: string;
        };
        if (!challenge_id || !public_key || !signature) return fail("bad_request", 400);

        const me = await linkedIdentity(admin, userId);
        if (!me) return fail("not_linked", 403);

        const consumed = await consumeChallenge(admin, challenge_id, "register", userId);
        if (!consumed) return fail("challenge_invalid", 400);

        const valid = await verifySignature(public_key, signature, signedMessage("register", challenge_id, consumed.challenge));
        if (!valid) {
          log("warn", "register_bad_signature", { role: me.role, code: me.code });
          return fail("bad_signature", 400);
        }

        const label = (device_label ?? "").toString().trim().slice(0, 60);
        const { data, error } = await admin
          .from("user_passkeys")
          .insert({ role: me.role, user_code: me.code, public_key, device_label: label || null })
          .select("id")
          .single();
        if (error) throw error;

        log("info", "passkey_registered", { role: me.role, code: me.code });
        return json({ ok: true, passkey_id: data.id });
      }

      case "login_challenge": {
        const { passkey_id } = body as { passkey_id?: string };
        if (!passkey_id) return fail("bad_request", 400);

        const { data: pk } = await admin
          .from("user_passkeys")
          .select("id")
          .eq("id", passkey_id)
          .is("revoked_at", null)
          .maybeSingle();
        if (!pk) return fail("invalid_passkey", 404);

        if ((await recentChallengeCount(admin, "passkey_id", passkey_id)) >= MAX_CHALLENGES_PER_WINDOW) {
          log("warn", "login_rate_limited", { passkey_id });
          return fail("rate_limited", 429);
        }

        const created = await createChallenge(admin, "login", userId, passkey_id);
        return json({ ok: true, challenge_id: created.challengeId, challenge: created.challenge });
      }

      case "login": {
        const { challenge_id, signature } = body as { challenge_id?: string; signature?: string };
        if (!challenge_id || !signature) return fail("bad_request", 400);

        const consumed = await consumeChallenge(admin, challenge_id, "login", userId);
        if (!consumed || !consumed.passkeyId) return fail("challenge_invalid", 400);

        const { data: pk } = await admin
          .from("user_passkeys")
          .select("id, role, user_code, public_key")
          .eq("id", consumed.passkeyId)
          .is("revoked_at", null)
          .maybeSingle();
        if (!pk) return fail("invalid_passkey", 404);

        const valid = await verifySignature(pk.public_key as string, signature, signedMessage("login", challenge_id, consumed.challenge));
        if (!valid) {
          log("warn", "login_bad_signature", { passkey_id: pk.id });
          return fail("bad_signature", 401);
        }

        const result = await linkAndIssueClaims(admin, userId, pk.role as Role, pk.user_code as string);
        if ("error" in result) return fail(result.error as string, result.status as number);

        await admin.from("user_passkeys").update({ last_used_at: new Date().toISOString() }).eq("id", pk.id);
        log("info", "passkey_login_success", { role: pk.role, code: pk.user_code });
        return json({ ok: true, role: result.role, profile: result.profile });
      }

      case "list": {
        const me = await linkedIdentity(admin, userId);
        if (!me) return fail("not_linked", 403);

        const { data, error } = await admin
          .from("user_passkeys")
          .select("id, device_label, created_at, last_used_at")
          .eq("role", me.role)
          .eq("user_code", me.code)
          .is("revoked_at", null)
          .order("created_at", { ascending: false });
        if (error) throw error;
        return json({ ok: true, passkeys: data ?? [] });
      }

      case "revoke": {
        const { passkey_id } = body as { passkey_id?: string };
        if (!passkey_id) return fail("bad_request", 400);

        const me = await linkedIdentity(admin, userId);
        if (!me) return fail("not_linked", 403);

        const { data, error } = await admin
          .from("user_passkeys")
          .update({ revoked_at: new Date().toISOString() })
          .eq("id", passkey_id)
          .eq("role", me.role)
          .eq("user_code", me.code)
          .is("revoked_at", null)
          .select("id")
          .maybeSingle();
        if (error) throw error;
        if (!data) return fail("invalid_passkey", 404);

        log("info", "passkey_revoked", { role: me.role, code: me.code });
        return json({ ok: true });
      }

      default:
        return fail("bad_request", 400);
    }
  } catch (e) {
    log("error", "passkey_action_failed", { action: body.action, error_message: String(e) });
    return fail("server_error", 500);
  }
});
