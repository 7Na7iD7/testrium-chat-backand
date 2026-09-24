import { serve } from "https://deno.land/std@0.224.0/http/server.ts";
import { createClient } from "https://esm.sh/@supabase/supabase-js@2";

const SUPABASE_URL = Deno.env.get("SUPABASE_URL")!;
const SERVICE_ROLE_KEY = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;
const ANON_KEY = Deno.env.get("SUPABASE_ANON_KEY")!;

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
    function: "submit-exam-result",
    timestamp: new Date().toISOString(),
    ...data,
  }));
}

const _questionsCache = new Map<string, Array<{ id: number; type: string; answer?: number; score: number }>>();
const _QUESTIONS_CACHE_MAX = 200;

async function loadQuestions(
  admin: ReturnType<typeof createClient>,
  currentFile: string,
): Promise<Array<{ id: number; type: string; answer?: number; score: number }> | null> {
  const cached = _questionsCache.get(currentFile);
  if (cached) {
    _questionsCache.delete(currentFile);
    _questionsCache.set(currentFile, cached);
    return cached;
  }

  const { data: fileBlob, error: downloadError } = await admin.storage
    .from("exam-content")
    .download(currentFile);

  if (downloadError || !fileBlob) {
    log("error", "storage_download_failed", { current_file: currentFile, storage_error: downloadError?.message });
    return null;
  }

  const content = JSON.parse(await fileBlob.text());
  const questions: Array<{ id: number; type: string; answer?: number; score: number }> = content.questions ?? [];

  if (_questionsCache.size >= _QUESTIONS_CACHE_MAX) {
    const oldestKey = _questionsCache.keys().next().value;
    if (oldestKey !== undefined) _questionsCache.delete(oldestKey);
  }
  _questionsCache.set(currentFile, questions);

  return questions;
}

interface IncomingAnswer {
  question_id: number;
  question_type: "multiple_choice" | "essay";
  selected_index: number | null;
  essay_text: string | null;
}

const SUBMIT_GRACE_MS = 3 * 60 * 1000;

function computeHardSubmitDeadline(exam: {
  available_until: string;
  duration_minutes: number | null;
  duration_policy: string | null;
  minutes_per_mcq: number | null;
  minutes_per_essay: number | null;
  timer_policy: string | null;
}, questions: Array<{ type: string }>): number {
  const availableUntilMs = new Date(exam.available_until).getTime();

  if (exam.timer_policy !== "from_student_start") {
    return availableUntilMs + SUBMIT_GRACE_MS;
  }

  let computedDurationMinutes = exam.duration_minutes ?? 0;
  if (exam.duration_policy === "auto_calculate") {
    const mcqCount = questions.filter((q) => q.type === "multiple_choice").length;
    const essayCount = questions.filter((q) => q.type === "essay").length;
    const perMcq = exam.minutes_per_mcq ?? 2.0;
    const perEssay = exam.minutes_per_essay ?? 4.0;
    computedDurationMinutes = Math.round(mcqCount * perMcq + essayCount * perEssay);
  }

  return availableUntilMs + computedDurationMinutes * 60 * 1000 + SUBMIT_GRACE_MS;
}

serve(async (req) => {
  const startedAt = Date.now();

  if (req.method === "OPTIONS") return new Response("ok", { headers: corsHeaders });

  const authHeader = req.headers.get("Authorization");
  if (!authHeader) {
    log("warn", "missing_auth_header");
    return json({ ok: false, message: "احراز هویت یافت نشد" }, 401);
  }

  const payload = await req.json().catch(() => null);
  if (!payload) {
    log("warn", "invalid_body");
    return json({ ok: false, message: "بدنه‌ی درخواست نامعتبر است" }, 400);
  }

  const { exam_id, start_time, end_time, answers } = payload as {
    exam_id?: string;
    start_time?: string | null;
    end_time?: string | null;
    answers?: IncomingAnswer[];
    // توجه: student_first_name/student_last_name/student_class دیگر از
    // payload خوانده نمی‌شوند — این فیلدها هویتی هستند و باید فقط از
    // رکورد تاییدشده‌ی سرور بیایند، نه از چیزی که کلاینت ارسال می‌کند
    // (وگرنه یک دانشجو می‌تواند اسم/کلاس دلخواه روی کارنامه‌اش ثبت کند).
  };

  if (!exam_id || !Array.isArray(answers)) {
    log("warn", "missing_params", { exam_id });
    return json({ ok: false, message: "پارامترها ناقص" }, 400);
  }

  const userClient = createClient(SUPABASE_URL, ANON_KEY, {
    global: { headers: { Authorization: authHeader } },
  });
  const { data: userData, error: userError } = await userClient.auth.getUser();
  if (userError || !userData?.user) {
    log("warn", "invalid_jwt", { auth_error: userError?.message });
    return json({ ok: false, message: "نشست نامعتبر است، دوباره وارد شو" }, 401);
  }
  const authUserId = userData.user.id;

  const admin = createClient(SUPABASE_URL, SERVICE_ROLE_KEY);

  const { data: studentRow, error: studentError } = await admin
    .from("students")
    .select("student_code, first_name, last_name, student_class, is_active")
    .eq("auth_user_id", authUserId)
    .maybeSingle();

  if (studentError) {
    log("error", "student_lookup_failed", { auth_user_id: authUserId, db_error: studentError.message });
  }
  if (!studentRow) {
    log("warn", "student_not_linked", { auth_user_id: authUserId });
    return json({ ok: false, message: "این حساب به هیچ دانشجویی متصل نیست" }, 403);
  }
  if (!studentRow.is_active) {
    log("warn", "inactive_student_submit", { student_code: studentRow.student_code });
    return json({ ok: false, message: "این کد غیرفعال است" }, 403);
  }
  const student_code = studentRow.student_code as string;
  // نام و کلاس همیشه از رکورد سرور گرفته می‌شود، هرگز از payload کلاینت.
  const student_first_name = studentRow.first_name as string;
  const student_last_name = studentRow.last_name as string;
  const student_class = studentRow.student_class as string;

  const { data: exam, error: examError } = await admin
    .from("exams")
    .select("current_file, available_from, available_until, duration_minutes, duration_policy, minutes_per_mcq, minutes_per_essay, timer_policy")
    .eq("exam_id", exam_id)
    .maybeSingle();

  if (examError || !exam) {
    log("error", "exam_not_found", { exam_id, student_code, db_error: examError?.message });
    return json({ ok: false, message: "آزمون یافت نشد" }, 404);
  }

  if (Date.now() < new Date(exam.available_from).getTime()) {
    log("warn", "submit_before_available_from", {
      exam_id,
      student_code,
      available_from: exam.available_from,
      now: new Date().toISOString(),
    });
    return json({ ok: false, error_code: "exam_not_started", message: "این آزمون هنوز شروع نشده است" }, 403);
  }

  const questions = await loadQuestions(admin, exam.current_file);
  if (questions === null) {
    return json({ ok: false, message: "خطا در بارگیری سوالات" }, 500);
  }

  const totalMcqScoreForExam = questions
    .filter((q) => q.type === "multiple_choice")
    .reduce((sum, q) => sum + (q.score ?? 0), 0);

  const { data: existing, error: existingError } = await admin
    .from("exam_sessions")
    .select("id, is_submitted, earned_score")
    .eq("exam_id", exam_id)
    .eq("student_code", student_code)
    .maybeSingle();

  if (existingError) {
    log("error", "existing_session_lookup_failed", { exam_id, student_code, db_error: existingError.message });
  }
  if (existing?.is_submitted) {
    log("info", "duplicate_submit_returned_previous_result", { exam_id, student_code });
    return json({
      ok: true,
      earned_mcq_score: existing.earned_score,
      total_mcq_score: totalMcqScoreForExam,
      already_submitted: true,
    });
  }

  const hardDeadline = computeHardSubmitDeadline(exam, questions);
  if (Date.now() > hardDeadline) {
    log("warn", "submit_after_hard_deadline", {
      exam_id,
      student_code,
      hard_deadline: new Date(hardDeadline).toISOString(),
      now: new Date().toISOString(),
    });
    return json({ ok: false, error_code: "exam_closed", message: "مهلت این آزمون به پایان رسیده است" }, 403);
  }

  let earnedMcqScore = 0;
  let totalMcqScore = 0;
  let correctCount = 0;
  let wrongCount = 0;
  let skippedCount = 0;
  let mcqTotal = 0;

  const gradedAnswers = answers.map((a) => {
    const q = questions.find((q) => q.id === a.question_id);
    if (!q) {
      return {
        question_id: a.question_id,
        question_type: a.question_type,
        selected_index: null,
        essay_text: null,
        is_correct: false,
        earned_score: 0,
        graded: false,
      };
    }

    if (q.type !== "multiple_choice") {
      return {
        question_id: a.question_id,
        question_type: "essay",
        selected_index: null,
        essay_text: a.essay_text ?? null,
        is_correct: false,
        earned_score: 0,
        graded: false,
      };
    }

    mcqTotal += 1;
    totalMcqScore += q.score ?? 0;
    const isCorrect = q.answer !== undefined && q.answer === a.selected_index;
    const earnedScore = isCorrect ? q.score ?? 0 : 0;
    earnedMcqScore += earnedScore;
    if (a.selected_index === null || a.selected_index === undefined) {
      skippedCount += 1;
    } else if (isCorrect) {
      correctCount += 1;
    } else {
      wrongCount += 1;
    }
    return {
      question_id: a.question_id,
      question_type: "multiple_choice",
      selected_index: a.selected_index,
      essay_text: null,
      is_correct: isCorrect,
      earned_score: earnedScore,
      graded: true,
    };
  });

  const hasEssay = questions.some((q) => q.type === "essay");

  const { data: rpcRows, error: rpcError } = await admin.rpc(
    "submit_exam_session_if_not_submitted",
    {
      p_exam_id: exam_id,
      p_student_code: student_code,
      p_student_first_name: student_first_name,
      p_student_last_name: student_last_name,
      p_student_class: student_class,
      p_start_time: start_time ?? null,
      p_end_time: end_time ?? null,
      p_answers: gradedAnswers,
      p_earned_score: earnedMcqScore,
      p_mcq_correct_count: correctCount,
      p_mcq_wrong_count: wrongCount,
      p_mcq_skipped_count: skippedCount,
      p_mcq_total_count: mcqTotal,
      p_is_fully_graded: !hasEssay,
    },
  );

  if (rpcError) {
    log("error", "submit_rpc_failed", {
      exam_id,
      student_code,
      db_error: rpcError.message,
      duration_ms: Date.now() - startedAt,
    });
    return json({ ok: false, message: "خطا در ذخیره‌ی نتیجه" }, 500);
  }

  const result = Array.isArray(rpcRows) ? rpcRows[0] : rpcRows;

  if (!result?.inserted) {
    log("warn", "submit_race_detected_returning_existing", { exam_id, student_code });
    return json({
      ok: true,
      earned_mcq_score: result?.earned_score ?? null,
      total_mcq_score: totalMcqScoreForExam,
      already_submitted: true,
    });
  }

  log("info", "submit_success", {
    exam_id,
    student_code,
    earned_mcq_score: earnedMcqScore,
    total_mcq_score: totalMcqScore,
    mcq_correct_count: correctCount,
    mcq_wrong_count: wrongCount,
    mcq_skipped_count: skippedCount,
    duration_ms: Date.now() - startedAt,
  });

  return json({ ok: true, earned_mcq_score: earnedMcqScore, total_mcq_score: totalMcqScore });
});
