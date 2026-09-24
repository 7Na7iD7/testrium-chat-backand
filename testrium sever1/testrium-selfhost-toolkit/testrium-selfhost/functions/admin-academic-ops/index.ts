import { serve } from "https://deno.land/std@0.224.0/http/server.ts";
import { createClient } from "https://esm.sh/@supabase/supabase-js@2";

const SUPABASE_URL = Deno.env.get("SUPABASE_URL")!;
const SERVICE_ROLE_KEY = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;

const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, x-client-info, apikey, content-type, x-admin-secret",
};

type AdminRole = "full" | "students" | "professors";

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
    function: "admin-academic-ops",
    timestamp: new Date().toISOString(),
    ...data,
  }));
}

async function sha256Hex(input: string): Promise<string> {
  const bytes = new TextEncoder().encode(input);
  const hashBuffer = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(hashBuffer))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function generateSecret(): string {
  const alphabet = "ABCDEFGHJKMNPQRSTUVWXYZabcdefghijkmnpqrstuvwxyz23456789";
  const groups = 5;
  const groupLen = 4;
  const bytes = new Uint8Array(groups * groupLen);
  crypto.getRandomValues(bytes);
  const parts: string[] = [];
  for (let g = 0; g < groups; g++) {
    let part = "";
    for (let i = 0; i < groupLen; i++) {
      part += alphabet[bytes[g * groupLen + i] % alphabet.length];
    }
    parts.push(part);
  }
  return parts.join("-");
}

interface AdminIdentity {
  id: string;
  full_name: string;
  role: AdminRole;
}

async function authenticateAdmin(
  admin: ReturnType<typeof createClient>,
  providedSecret: string,
): Promise<AdminIdentity | null> {
  if (!providedSecret) return null;

  const secretHash = await sha256Hex(providedSecret.trim());

  const { data, error } = await admin
    .from("admins")
    .select("id, full_name, role, is_active")
    .eq("secret_hash", secretHash)
    .maybeSingle();

  if (error) {
    log("error", "admin_lookup_failed", { db_error: error.message });
    return null;
  }
  if (!data || !data.is_active) return null;

  await admin.from("admins").update({ last_used_at: new Date().toISOString() }).eq("id", data.id);

  return { id: data.id as string, full_name: data.full_name as string, role: data.role as AdminRole };
}

const RATE_LIMIT_WINDOW_MINUTES = 10;
const RATE_LIMIT_MAX_ATTEMPTS = 5;

async function isRateLimited(admin: ReturnType<typeof createClient>, ip: string): Promise<boolean> {
  const since = new Date(Date.now() - RATE_LIMIT_WINDOW_MINUTES * 60 * 1000).toISOString();
  const { count, error } = await admin
    .from("admin_audit_log")
    .select("id", { count: "exact", head: true })
    .eq("ip_address", ip)
    .in("result", ["unauthorized", "blocked"])
    .gte("created_at", since);

  if (error) {
    log("error", "rate_limit_check_failed", { ip, db_error: error.message });
    return false;
  }
  return (count ?? 0) >= RATE_LIMIT_MAX_ATTEMPTS;
}

function getClientIp(req: Request): string {
  return req.headers.get("x-forwarded-for")?.split(",")[0]?.trim() ?? "unknown";
}

function summarizePayload(payload: Record<string, unknown>): Record<string, unknown> {
  const allow = ["section_id", "professor_id", "professor_code", "semester_id", "course_id", "offering_id", "is_active", "role", "code", "admin_id", "full_name", "limit", "offset"];
  const summary: Record<string, unknown> = {};
  for (const key of allow) {
    if (key in payload) summary[key] = payload[key];
  }
  if (Array.isArray((payload as any).rows)) {
    summary.rows_count = (payload as any).rows.length;
  }
  return summary;
}

const ACTION_ROLES: Record<string, AdminRole[] | "full"> = {
  create_semester: "full",
  set_semester_active: "full",
  delete_semester: "full",
  create_offering: "full",
  create_section: "full",
  update_section: "full",
  delete_section: "full",
  delete_course: "full",
  create_admin: "full",
  list_admins: "full",
  set_admin_active: "full",
  list_audit_log: "full",

  list_sections: ["full", "students", "professors"],
  list_section_students: ["full", "students"],
  bulk_enroll: ["full", "students"],
  enroll_students: ["full", "students"],

  create_professor: ["full", "professors"],
  delete_professor: ["full", "professors"],
  set_professor_active: ["full", "professors"],
  list_professors: ["full", "professors"],
  bulk_create_professors: ["full", "professors"],

  allow_relink: ["full", "students", "professors"],
  whoami: ["full", "students", "professors"],
};

function isActionAllowed(action: string, identity: AdminIdentity, payload: Record<string, unknown>): boolean {
  if (identity.role === "full") return true;

  const rule = ACTION_ROLES[action];
  if (rule === "full") return false;
  if (!rule) return false;
  if (!rule.includes(identity.role)) return false;

  if (action === "allow_relink") {
    const targetRole = (payload as { role?: string }).role;
    if (identity.role === "students" && targetRole !== "student") return false;
    if (identity.role === "professors" && targetRole !== "professor") return false;
  }

  return true;
}

serve(async (req) => {
  const startedAt = Date.now();

  if (req.method === "OPTIONS") return new Response("ok", { headers: corsHeaders });

  const admin = createClient(SUPABASE_URL, SERVICE_ROLE_KEY);
  const ip = getClientIp(req);

  if (await isRateLimited(admin, ip)) {
    log("warn", "rate_limited", { ip });
    await admin.from("admin_audit_log").insert({ action: "n/a", result: "blocked", ip_address: ip });
    return json({ ok: false, message: "تعداد تلاش‌های ناموفق بیش از حد مجاز — کمی بعد دوباره امتحان کن" }, 429);
  }

  const providedSecret = req.headers.get("x-admin-secret") ?? "";
  const identity = await authenticateAdmin(admin, providedSecret);
  if (!identity) {
    log("warn", "unauthorized_attempt", { ip });
    await admin.from("admin_audit_log").insert({ action: "n/a", result: "unauthorized", ip_address: ip });
    return json({ ok: false, message: "دسترسی غیرمجاز" }, 401);
  }

  const body = await req.json().catch(() => null);
  if (!body || typeof body.action !== "string") {
    log("warn", "invalid_body", { ip });
    return json({ ok: false, message: "بدنه‌ی درخواست نامعتبر است" }, 400);
  }

  const { action, payload } = body as { action: string; payload: Record<string, unknown> };

  if (!isActionAllowed(action, identity, payload ?? {})) {
    log("warn", "forbidden_by_role", { action, ip, admin_id: identity.id, admin_role: identity.role });
    await admin.from("admin_audit_log").insert({
      action,
      payload_summary: summarizePayload(payload ?? {}),
      result: "failure",
      error_message: "forbidden_by_role",
      ip_address: ip,
      admin_id: identity.id,
    });
    return json({ ok: false, message: "این عملیات خارج از دسترسی نقش شماست" }, 403);
  }

  log("info", "action_received", { action, ip, admin_id: identity.id, admin_name: identity.full_name, admin_role: identity.role });

  let response: Response;
  let auditResult: "success" | "failure" = "success";
  let auditError: string | null = null;

  try {
    switch (action) {
      case "whoami": {
        response = json({ ok: true, admin: { id: identity.id, full_name: identity.full_name, role: identity.role } });
        break;
      }

      case "create_semester": {
        const { title, starts_at, ends_at } = payload as {
          title: string; starts_at: string; ends_at: string;
        };
        const { data, error } = await admin
          .from("semesters")
          .insert({ title, starts_at, ends_at, is_active: true })
          .select()
          .single();
        if (error) throw error;
        log("info", "action_success", { action, duration_ms: Date.now() - startedAt });
        response = json({ ok: true, semester: data });
        break;
      }

      case "set_semester_active": {
        const { semester_id, is_active } = payload as { semester_id: string; is_active: boolean };
        const { error } = await admin.from("semesters").update({ is_active }).eq("semester_id", semester_id);
        if (error) throw error;
        log("info", "action_success", { action, semester_id, duration_ms: Date.now() - startedAt });
        response = json({ ok: true });
        break;
      }

      case "delete_semester": {
        const { semester_id } = payload as { semester_id?: string };
        if (!semester_id) { response = json({ ok: false, message: "ترم مشخص نشده" }, 400); auditResult = "failure"; break; }

        const { error } = await admin.from("semesters").delete().eq("semester_id", semester_id);
        if (error) {
          if ((error as { code?: string }).code === "23503") {
            log("warn", "delete_blocked_fk", { action, semester_id });
            response = json({ ok: false, message: "این ترم آزمون یا داده‌ی وابسته دارد و نمی‌شود حذفش کرد" }, 409);
            auditResult = "failure";
            break;
          }
          throw error;
        }
        log("info", "action_success", { action, semester_id, duration_ms: Date.now() - startedAt });
        response = json({ ok: true });
        break;
      }

      case "create_offering": {
        const { course_title, course_code, credit_units, semester_id } = payload as {
          course_title: string; course_code?: string; credit_units?: number; semester_id: string;
        };

        if (!course_title?.trim()) { response = json({ ok: false, message: "عنوان درس الزامی است" }, 400); auditResult = "failure"; break; }
        if (!course_code?.trim()) { response = json({ ok: false, message: "کد درس الزامی است" }, 400); auditResult = "failure"; break; }
        const units = Number(credit_units);
        if (!Number.isInteger(units) || units < 0 || units > 3) {
          response = json({ ok: false, message: "تعداد واحد باید عددی بین ۰ تا ۳ باشد" }, 400);
          auditResult = "failure";
          break;
        }

        let courseId: string;
        const existing = await admin
          .from("course_catalog")
          .select("course_id")
          .eq("course_title", course_title)
          .maybeSingle();
        if (existing.data) {
          courseId = existing.data.course_id as string;
          const { error: updErr } = await admin
            .from("course_catalog")
            .update({ course_code: course_code.trim(), credit_units: units })
            .eq("course_id", courseId);
          if (updErr) throw updErr;
        } else {
          const { data, error } = await admin
            .from("course_catalog")
            .insert({ course_title, course_code: course_code.trim(), credit_units: units })
            .select("course_id")
            .single();
          if (error) throw error;
          courseId = data.course_id as string;
        }
        const { data: offering, error: offErr } = await admin
          .from("course_offerings")
          .upsert({ course_id: courseId, semester_id }, { onConflict: "course_id,semester_id" })
          .select()
          .single();
        if (offErr) throw offErr;
        log("info", "action_success", { action, course_id: courseId, duration_ms: Date.now() - startedAt });
        response = json({ ok: true, offering });
        break;
      }

      case "create_section": {
        const { offering_id, professor_code, section_label, capacity } = payload as {
          offering_id: string; professor_code: string; section_label: string; capacity?: number;
        };
        const prof = await admin
          .from("professors")
          .select("id")
          .eq("professor_code", professor_code)
          .maybeSingle();
        if (!prof.data) { response = json({ ok: false, message: "استاد با این کد یافت نشد" }, 404); auditResult = "failure"; break; }

        const { data, error } = await admin
          .from("sections")
          .insert({
            offering_id,
            professor_id: prof.data.id,
            section_label,
            capacity: capacity ?? null,
          })
          .select()
          .single();
        if (error) throw error;
        log("info", "action_success", { action, offering_id, duration_ms: Date.now() - startedAt });
        response = json({ ok: true, section: data });
        break;
      }

      case "list_sections": {
        const { data, error } = await admin
          .from("sections")
          .select(`
            section_id, section_label, capacity, created_at,
            professors ( id, professor_code, full_name ),
            course_offerings (
              offering_id,
              course_catalog ( course_title, course_code ),
              semesters ( title, is_active )
            )
          `)
          .order("created_at", { ascending: false });
        if (error) throw error;

        const sectionIds = (data ?? []).map((s: any) => s.section_id as string);
        const counts: Record<string, number> = {};

        if (sectionIds.length > 0) {
          const { data: enrollRows, error: enrErr } = await admin
            .from("enrollments")
            .select("section_id")
            .in("section_id", sectionIds)
            .eq("status", "enrolled");
          if (enrErr) throw enrErr;
          for (const row of enrollRows ?? []) {
            const sid = row.section_id as string;
            counts[sid] = (counts[sid] ?? 0) + 1;
          }
        }

        const sections = (data ?? []).map((s: any) => ({
          ...s,
          student_count: counts[s.section_id] ?? 0,
        }));

        log("info", "action_success", { action, section_count: sections.length, duration_ms: Date.now() - startedAt });
        response = json({ ok: true, sections });
        break;
      }

      case "list_section_students": {
        const { section_id } = payload as { section_id?: string };
        if (!section_id) { response = json({ ok: false, message: "سکشن مشخص نشده" }, 400); auditResult = "failure"; break; }

        const { data, error } = await admin
          .from("enrollments")
          .select("student_code, status, students(first_name, last_name, student_class)")
          .eq("section_id", section_id)
          .eq("status", "enrolled");
        if (error) throw error;

        const students = (data ?? []).map((r: any) => ({
          student_code: r.student_code,
          first_name: r.students?.first_name ?? "",
          last_name: r.students?.last_name ?? "",
          student_class: r.students?.student_class ?? "",
        }));
        log("info", "action_success", { action, section_id, student_count: students.length, duration_ms: Date.now() - startedAt });
        response = json({ ok: true, students });
        break;
      }

      case "update_section": {
        const { section_id, section_label, capacity, professor_code } = payload as {
          section_id?: string;
          section_label?: string;
          capacity?: number | null;
          professor_code?: string;
        };
        if (!section_id) { response = json({ ok: false, message: "سکشن مشخص نشده" }, 400); auditResult = "failure"; break; }

        const updateBody: Record<string, unknown> = {};

        if (section_label !== undefined && section_label !== null) {
          const trimmed = section_label.trim();
          if (!trimmed) { response = json({ ok: false, message: "نام گروه نمی‌تواند خالی باشد" }, 400); auditResult = "failure"; break; }
          updateBody.section_label = trimmed;
        }

        if (capacity !== undefined) {
          updateBody.capacity = capacity;
        }

        if (professor_code) {
          const prof = await admin
            .from("professors")
            .select("id")
            .eq("professor_code", professor_code.trim())
            .maybeSingle();
          if (!prof.data) { response = json({ ok: false, message: "استاد با این کد یافت نشد" }, 404); auditResult = "failure"; break; }
          updateBody.professor_id = prof.data.id;
        }

        if (Object.keys(updateBody).length === 0) {
          response = json({ ok: false, message: "چیزی برای بروزرسانی ارسال نشده" }, 400);
          auditResult = "failure";
          break;
        }

        const { data, error } = await admin
          .from("sections")
          .update(updateBody)
          .eq("section_id", section_id)
          .select()
          .single();
        if (error) throw error;
        log("info", "action_success", { action, section_id, duration_ms: Date.now() - startedAt });
        response = json({ ok: true, section: data });
        break;
      }

      case "delete_section": {
        const { section_id } = payload as { section_id?: string };
        if (!section_id) { response = json({ ok: false, message: "کلاس مشخص نشده" }, 400); auditResult = "failure"; break; }

        const { error } = await admin.from("sections").delete().eq("section_id", section_id);
        if (error) {
          if ((error as { code?: string }).code === "23503") {
            log("warn", "delete_blocked_fk", { action, section_id });
            response = json({ ok: false, message: "این کلاس آزمون وابسته دارد و نمی‌شود حذفش کرد" }, 409);
            auditResult = "failure";
            break;
          }
          throw error;
        }
        log("info", "action_success", { action, section_id, duration_ms: Date.now() - startedAt });
        response = json({ ok: true });
        break;
      }

      case "bulk_enroll": {
        const rows = (payload.rows as Array<{ student_code: string; section_id: string }>) ?? [];
        if (rows.length === 0) { response = json({ ok: false, message: "لیست خالی است" }, 400); auditResult = "failure"; break; }

        const records = rows.map((r) => ({
          section_id: r.section_id,
          student_code: r.student_code,
          status: "enrolled",
        }));
        const { error, count } = await admin
          .from("enrollments")
          .upsert(records, { onConflict: "section_id,student_code", count: "exact" });
        if (error) throw error;
        log("info", "action_success", { action, inserted: count ?? records.length, duration_ms: Date.now() - startedAt });
        response = json({ ok: true, inserted: count ?? records.length });
        break;
      }

      case "enroll_students": {
        const { section_id, rows } = payload as {
          section_id: string;
          rows: Array<{
            student_code: string;
            first_name: string;
            last_name: string;
            student_class?: string;
          }>;
        };

        if (!section_id) { response = json({ ok: false, message: "سکشن انتخاب نشده" }, 400); auditResult = "failure"; break; }
        if (!rows || rows.length === 0) { response = json({ ok: false, message: "حداقل یک دانشجو وارد کن" }, 400); auditResult = "failure"; break; }

        const cleanRows = rows
          .map((r) => ({
            student_code: (r.student_code ?? "").trim(),
            first_name: (r.first_name ?? "").trim(),
            last_name: (r.last_name ?? "").trim(),
            student_class: (r.student_class ?? "").trim(),
          }))
          .filter((r) => r.student_code && r.first_name && r.last_name);

        if (cleanRows.length === 0) {
          response = json({ ok: false, message: "هیچ ردیف معتبری پیدا نشد (کد/نام/نام‌خانوادگی خالی)" }, 400);
          auditResult = "failure";
          break;
        }

        const { data: existingStudents, error: existingLookupErr } = await admin
          .from("students")
          .select("student_code, first_name, last_name")
          .in("student_code", cleanRows.map((r) => r.student_code));
        if (existingLookupErr) throw existingLookupErr;

        const existingByCode = new Map((existingStudents ?? []).map((s) => [s.student_code, s]));
        const conflicts = cleanRows
          .filter((r) => {
            const existing = existingByCode.get(r.student_code);
            return existing && (existing.first_name !== r.first_name || existing.last_name !== r.last_name);
          })
          .map((r) => {
            const existing = existingByCode.get(r.student_code)!;
            return `کد ${r.student_code}: قبلاً برای «${existing.first_name} ${existing.last_name}» ثبت شده، الان داری «${r.first_name} ${r.last_name}» می‌فرستی`;
          });

        if (conflicts.length > 0) {
          log("warn", "student_code_identity_conflict", { action, section_id, conflicts });
          response = json({
            ok: false,
            error_code: "student_code_conflict",
            message: `تداخل کد دانشجویی — این کدها قبلاً برای فرد دیگری ثبت شده‌اند:\n${conflicts.join("\n")}\nاگه همون فرده، اول ویرایش کن؛ اگه فرد جدیده، کد دیگه‌ای استفاده کن.`,
          }, 409);
          auditResult = "failure";
          break;
        }

        const studentRecords = cleanRows.map((r) => ({
          student_code: r.student_code,
          first_name: r.first_name,
          last_name: r.last_name,
          student_class: r.student_class,
          is_active: true,
        }));
        const { error: stuErr } = await admin
          .from("students")
          .upsert(studentRecords, { onConflict: "student_code" });
        if (stuErr) throw stuErr;

        const enrollRecords = cleanRows.map((r) => ({
          section_id,
          student_code: r.student_code,
          status: "enrolled",
        }));
        const { error: enrErr, count } = await admin
          .from("enrollments")
          .upsert(enrollRecords, { onConflict: "section_id,student_code", count: "exact" });
        if (enrErr) throw enrErr;

        log("info", "action_success", { action, section_id, inserted: count ?? enrollRecords.length, duration_ms: Date.now() - startedAt });
        response = json({ ok: true, inserted: count ?? enrollRecords.length });
        break;
      }

      case "create_professor": {
        const { professor_code, full_name } = payload as {
          professor_code: string; full_name: string;
        };
        if (!professor_code?.trim() || !full_name?.trim()) {
          response = json({ ok: false, message: "کد استاد و نام کامل الزامی است" }, 400);
          auditResult = "failure";
          break;
        }

        const existing = await admin
          .from("professors")
          .select("id")
          .eq("professor_code", professor_code.trim())
          .maybeSingle();
        if (existing.data) {
          response = json({ ok: false, message: "استادی با این کد قبلاً ثبت شده است" }, 409);
          auditResult = "failure";
          break;
        }

        const { data, error } = await admin
          .from("professors")
          .insert({
            professor_code: professor_code.trim(),
            full_name: full_name.trim(),
            is_active: true,
          })
          .select()
          .single();
        if (error) throw error;
        log("info", "action_success", { action, professor_code: professor_code.trim(), duration_ms: Date.now() - startedAt });
        response = json({ ok: true, professor: data });
        break;
      }

      case "delete_course": {
        const { course_id } = payload as { course_id?: string };
        if (!course_id) { response = json({ ok: false, message: "درس مشخص نشده" }, 400); auditResult = "failure"; break; }

        const { error } = await admin.from("course_catalog").delete().eq("course_id", course_id);
        if (error) {
          if ((error as { code?: string }).code === "23503") {
            log("warn", "delete_blocked_fk", { action, course_id });
            response = json({ ok: false, message: "این درس آزمون یا داده‌ی وابسته دارد و نمی‌شود حذفش کرد" }, 409);
            auditResult = "failure";
            break;
          }
          throw error;
        }
        log("info", "action_success", { action, course_id, duration_ms: Date.now() - startedAt });
        response = json({ ok: true });
        break;
      }

      case "delete_professor": {
        const { professor_id } = payload as { professor_id?: string };
        if (!professor_id) { response = json({ ok: false, message: "استاد مشخص نشده" }, 400); auditResult = "failure"; break; }

        const { error } = await admin.from("professors").delete().eq("id", professor_id);
        if (error) {
          if ((error as { code?: string }).code === "23503") {
            log("warn", "delete_blocked_fk", { action, professor_id });
            response = json({
              ok: false,
              message: "این استاد گروه/سکشن فعال دارد و نمی‌شود حذفش کرد — اول سکشن‌هایش را جابه‌جا یا حذف کن",
            }, 409);
            auditResult = "failure";
            break;
          }
          throw error;
        }
        log("info", "action_success", { action, professor_id, duration_ms: Date.now() - startedAt });
        response = json({ ok: true });
        break;
      }

      case "set_professor_active": {
        const { professor_id, is_active } = payload as { professor_id: string; is_active: boolean };
        const { error } = await admin.from("professors").update({ is_active }).eq("id", professor_id);
        if (error) throw error;
        log("info", "action_success", { action, professor_id, is_active, duration_ms: Date.now() - startedAt });
        response = json({ ok: true });
        break;
      }

      case "list_professors": {
        const { data, error } = await admin
          .from("professors")
          .select("id, professor_code, full_name, is_active, created_at")
          .order("full_name", { ascending: true });
        if (error) throw error;
        log("info", "action_success", { action, professor_count: data?.length ?? 0, duration_ms: Date.now() - startedAt });
        response = json({ ok: true, professors: data });
        break;
      }

      case "bulk_create_professors": {
        const rows = (payload.rows as Array<{ professor_code: string; full_name: string }>) ?? [];
        if (rows.length === 0) { response = json({ ok: false, message: "لیست خالی است" }, 400); auditResult = "failure"; break; }

        const records = rows
          .filter((r) => r.professor_code?.trim() && r.full_name?.trim())
          .map((r) => ({
            professor_code: r.professor_code.trim(),
            full_name: r.full_name.trim(),
            is_active: true,
          }));
        if (records.length === 0) { response = json({ ok: false, message: "هیچ ردیف معتبری پیدا نشد" }, 400); auditResult = "failure"; break; }

        const { error, count } = await admin
          .from("professors")
          .upsert(records, { onConflict: "professor_code", count: "exact" });
        if (error) throw error;
        log("info", "action_success", { action, inserted: count ?? records.length, duration_ms: Date.now() - startedAt });
        response = json({ ok: true, inserted: count ?? records.length });
        break;
      }

      case "allow_relink": {
        const { role, code } = payload as { role?: "student" | "professor"; code?: string };
        if (role !== "student" && role !== "professor") {
          response = json({ ok: false, message: "role باید student یا professor باشد" }, 400);
          auditResult = "failure";
          break;
        }
        if (!code?.trim()) {
          response = json({ ok: false, message: "کد مشخص نشده" }, 400);
          auditResult = "failure";
          break;
        }

        const table = role === "student" ? "students" : "professors";
        const column = role === "student" ? "student_code" : "professor_code";
        const relinkAllowedUntil = new Date(Date.now() + 10 * 60 * 1000).toISOString();

        const { data, error } = await admin
          .from(table)
          .update({ relink_allowed_until: relinkAllowedUntil })
          .eq(column, code.trim())
          .select(column)
          .maybeSingle();
        if (error) throw error;
        if (!data) {
          response = json({ ok: false, message: "کدی با این مقدار یافت نشد" }, 404);
          auditResult = "failure";
          break;
        }

        log("info", "action_success", { action, role, code: code.trim(), relink_allowed_until: relinkAllowedUntil, duration_ms: Date.now() - startedAt });
        response = json({ ok: true, relink_allowed_until: relinkAllowedUntil });
        break;
      }

      case "create_admin": {
        const { full_name, role: newRole } = payload as { full_name?: string; role?: AdminRole };
        if (!full_name?.trim()) {
          response = json({ ok: false, message: "نام الزامی است" }, 400);
          auditResult = "failure";
          break;
        }
        const roleValue: AdminRole = newRole === "students" || newRole === "professors" ? newRole : "full";

        const newSecret = generateSecret();
        const newHash = await sha256Hex(newSecret);

        const { data, error } = await admin
          .from("admins")
          .insert({ full_name: full_name.trim(), secret_hash: newHash, role: roleValue, is_active: true })
          .select("id, full_name, role, created_at")
          .single();
        if (error) throw error;

        log("info", "action_success", { action, new_admin_id: data.id, new_admin_name: data.full_name, new_admin_role: roleValue, duration_ms: Date.now() - startedAt });
        response = json({ ok: true, admin: data, secret: newSecret });
        break;
      }

      case "list_admins": {
        const { data, error } = await admin
          .from("admins")
          .select("id, full_name, role, is_active, created_at, last_used_at")
          .order("created_at", { ascending: true });
        if (error) throw error;

        const admins = data ?? [];
        const counts = await Promise.all(
          admins.map(async (a: any) => {
            const [totalRes, successRes, failRes] = await Promise.all([
              admin.from("admin_audit_log").select("id", { count: "exact", head: true }).eq("admin_id", a.id),
              admin.from("admin_audit_log").select("id", { count: "exact", head: true }).eq("admin_id", a.id).eq("result", "success"),
              admin.from("admin_audit_log").select("id", { count: "exact", head: true }).eq("admin_id", a.id).eq("result", "failure"),
            ]);
            return {
              total: totalRes.count ?? 0,
              success: successRes.count ?? 0,
              failure: failRes.count ?? 0,
            };
          }),
        );

        const adminsWithStats = admins.map((a: any, i: number) => ({
          ...a,
          action_count: counts[i].total,
          success_count: counts[i].success,
          failure_count: counts[i].failure,
        }));

        log("info", "action_success", { action, admin_count: admins.length, duration_ms: Date.now() - startedAt });
        response = json({ ok: true, admins: adminsWithStats });
        break;
      }

      case "set_admin_active": {
        const { admin_id, is_active } = payload as { admin_id?: string; is_active?: boolean };
        if (!admin_id) { response = json({ ok: false, message: "ادمین مشخص نشده" }, 400); auditResult = "failure"; break; }
        if (admin_id === identity.id && is_active === false) {
          response = json({ ok: false, message: "نمی‌توانی خودت را غیرفعال کنی" }, 400);
          auditResult = "failure";
          break;
        }

        const { error } = await admin.from("admins").update({ is_active }).eq("id", admin_id);
        if (error) throw error;
        log("info", "action_success", { action, target_admin_id: admin_id, is_active, duration_ms: Date.now() - startedAt });
        response = json({ ok: true });
        break;
      }

      case "list_audit_log": {
        const { limit, offset, admin_id } = payload as { limit?: number; offset?: number; admin_id?: string };
        const pageSize = Math.min(Math.max(limit ?? 50, 1), 200);
        const pageOffset = Math.max(offset ?? 0, 0);

        let query = admin
          .from("admin_audit_log")
          .select("id, action, result, error_message, ip_address, created_at, admins(id, full_name)", { count: "exact" })
          .order("created_at", { ascending: false })
          .range(pageOffset, pageOffset + pageSize - 1);

        if (admin_id) {
          query = query.eq("admin_id", admin_id);
        }

        const { data, error, count } = await query;
        if (error) throw error;

        const entries = (data ?? []).map((r: any) => ({
          id: r.id,
          action: r.action,
          result: r.result,
          error_message: r.error_message,
          ip_address: r.ip_address,
          created_at: r.created_at,
          admin_name: r.admins?.full_name ?? null,
        }));

        log("info", "action_success", { action, returned: entries.length, filtered_admin_id: admin_id ?? null, duration_ms: Date.now() - startedAt });
        response = json({ ok: true, entries, total: count ?? entries.length });
        break;
      }

      default:
        log("warn", "unknown_action", { action });
        response = json({ ok: false, message: `عملیات نامعتبر: ${action}` }, 400);
        auditResult = "failure";
    }
  } catch (e) {
    log("error", "action_failed", {
      action,
      error_message: String(e),
      duration_ms: Date.now() - startedAt,
    });
    response = json({ ok: false, message: `خطا: ${e}` }, 500);
    auditResult = "failure";
    auditError = String(e);
  }

  if (action !== "whoami") {
    await admin.from("admin_audit_log").insert({
      action,
      payload_summary: summarizePayload(payload ?? {}),
      result: auditResult,
      error_message: auditError,
      ip_address: ip,
      admin_id: identity.id,
    });
  }

  return response;
});
