-- ====================================================================
-- Testrium — Migration 0003: معماری ترم / درس‌ارائه‌شده / گروه (Section)
-- ====================================================================
-- هدف: رفع دو مشکل ریشه‌ای در معماری فعلی:
--   (الف) نشتی امنیتی: policy فعلی exams_select_public به هر کاربری
--         اجازه‌ی خواندن همه‌ی آزمون‌های همه‌ی دروس را می‌دهد.
--   (ب)  نبود مفهوم Semester / Section: امکان نداشتن دو گروه برای یک
--         درس، و نبود جداسازی انتخاب‌واحد بین ترم‌ها.
--
-- استراتژی migrate: additive و بدون حذف داده. جداول قدیمی (courses,
-- course_enrollments, exams.course_id) دست‌نخورده باقی می‌مانند تا
-- کد فعلی اپ نشکند؛ جداول/ستون‌های جدید کنارشان اضافه می‌شوند و در
-- پایان یک migration داده انجام می‌شود. جابه‌جایی نهایی کد اپ به
-- section_id در فاز بعدی (بعد از تایید دیتا) انجام می‌شود.
-- ====================================================================

-- ── ۱. semesters ──
create table if not exists public.semesters (
  semester_id   uuid primary key default gen_random_uuid(),
  title         text not null,                -- "نیم‌سال اول ۱۴۰۴-۱۴۰۵"
  starts_at     timestamptz not null,
  ends_at       timestamptz not null,
  is_active     boolean not null default true, -- ترم جاری/قابل‌مشاهده
  created_at    timestamptz not null default now()
);

-- فقط یک ترم می‌تواند به‌طور هم‌زمان «ترم جاری پیش‌فرض» باشد (اختیاری،
-- برای راحتی UI ادمین؛ چند ترم می‌توانند is_active=true باشند اگر
-- overlap لازم است، اما معمولاً یکی است)
create index if not exists idx_semesters_active on public.semesters(is_active);

alter table public.semesters enable row level security;

-- خواندن عمومی (برای نمایش نام ترم در پنل‌ها) — نوشتن فقط service_role
create policy "semesters_select_all"
  on public.semesters for select
  using (true);


-- ── ۲. course_catalog ──
-- کاتالوگ ثابت درس (مستقل از استاد/ترم). courses قدیمی در واقع همین
-- نقش را با آلودگی به professor_id بازی می‌کرد؛ اینجا آن را خالص می‌کنیم.
create table if not exists public.course_catalog (
  course_id     uuid primary key default gen_random_uuid(),
  course_code   text unique,        -- مثلا "CE-301" (اختیاری)
  course_title  text not null,
  created_at    timestamptz not null default now()
);

alter table public.course_catalog enable row level security;

create policy "course_catalog_select_all"
  on public.course_catalog for select
  using (true);


-- ── ۳. course_offerings ──
-- «درس X در ترم Y» — هنوز گروه/استاد مشخص نیست.
create table if not exists public.course_offerings (
  offering_id   uuid primary key default gen_random_uuid(),
  course_id     uuid not null references public.course_catalog(course_id) on delete cascade,
  semester_id   uuid not null references public.semesters(semester_id) on delete cascade,
  created_at    timestamptz not null default now(),
  unique (course_id, semester_id)
);

create index if not exists idx_offerings_semester on public.course_offerings(semester_id);

alter table public.course_offerings enable row level security;

create policy "course_offerings_select_all"
  on public.course_offerings for select
  using (true);


-- ── ۴. sections ──
-- گروه درسی مشخص: یک استاد + ظرفیت + برچسب گروه، زیر یک offering.
-- این همان چیزی است که الان کاملاً در پروژه غایب است.
create table if not exists public.sections (
  section_id     uuid primary key default gen_random_uuid(),
  offering_id    uuid not null references public.course_offerings(offering_id) on delete cascade,
  professor_id   uuid not null references public.professors(id) on delete restrict,
  section_label  text not null default 'گروه ۱',   -- "گروه ۱ (خواهران)"
  capacity       integer,
  created_at     timestamptz not null default now()
);

create index if not exists idx_sections_offering on public.sections(offering_id);
create index if not exists idx_sections_professor on public.sections(professor_id);

alter table public.sections enable row level security;

-- استاد فقط سکشن‌های خودش را مدیریت می‌کند
create policy "sections_manage_own"
  on public.sections for all
  using (professor_id in (select id from public.professors where auth_user_id = auth.uid()))
  with check (professor_id in (select id from public.professors where auth_user_id = auth.uid()));

-- توجه: policy مربوط به «دانشجوی enrolled بتواند section خودش را
-- بخواند» بعد از ساخت جدول enrollments (بخش ۵) تعریف می‌شود، چون به
-- آن جدول رفرنس می‌دهد و enrollments هنوز اینجا ساخته نشده.


-- ── ۵. enrollments (جایگزین course_enrollments، به‌ازای هر ترم) ──
create table if not exists public.enrollments (
  enrollment_id  uuid primary key default gen_random_uuid(),
  section_id     uuid not null references public.sections(section_id) on delete cascade,
  student_code   text not null references public.students(student_code) on delete cascade,
  status         text not null default 'enrolled'
                 check (status in ('enrolled', 'dropped', 'passed')),
  created_at     timestamptz not null default now(),
  unique (section_id, student_code)
);

create index if not exists idx_enrollments_student on public.enrollments(student_code);
create index if not exists idx_enrollments_section on public.enrollments(section_id);

alter table public.enrollments enable row level security;

-- دانشجو فقط enrollment های خودش را می‌بیند
create policy "enrollments_select_own"
  on public.enrollments for select
  using (
    student_code = (select student_code from public.students where auth_user_id = auth.uid())
  );

-- استاد enrollment های سکشن‌های خودش را می‌بیند (برای لیست کلاس)
create policy "enrollments_select_professor_own_section"
  on public.enrollments for select
  using (
    section_id in (
      select section_id from public.sections
      where professor_id in (select id from public.professors where auth_user_id = auth.uid())
    )
  );

-- نوشتن/ویرایش enrollment فقط از طریق service_role (پنل مدیریت،
-- عملیات انتخاب‌واحد) — بدون policy برای anon/authenticated یعنی
-- کلاینت هرگز مستقیم نمی‌تواند enrollment بسازد یا حذف کند.

-- حالا که enrollments وجود دارد، policy معلق بخش ۴ را همین‌جا تعریف
-- می‌کنیم: دانشجویانی که در یک section، enrolled هستند بتوانند
-- متادیتای همان section (مثلاً نام گروه) را بخوانند.
create policy "sections_select_enrolled_student"
  on public.sections for select
  using (
    section_id in (
      select section_id from public.enrollments e
      join public.students s on s.student_code = e.student_code
      where s.auth_user_id = auth.uid() and e.status = 'enrolled'
    )
  );


-- ── ۶. اتصال exams به sections (به‌جای courses) ──
alter table public.exams
  add column if not exists section_id uuid references public.sections(section_id);

create index if not exists idx_exams_section on public.exams(section_id);

-- policy جدید: استاد فقط برای سکشن خودش آزمون بسازد/ویرایش کند
create policy "exams_manage_own_section"
  on public.exams for all
  using (
    section_id in (
      select section_id from public.sections
      where professor_id in (select id from public.professors where auth_user_id = auth.uid())
    )
  )
  with check (
    section_id in (
      select section_id from public.sections
      where professor_id in (select id from public.professors where auth_user_id = auth.uid())
    )
  );

-- ⚠️ رفع نشتی امنیتی: policy عمومی/آزاد قبلی حذف می‌شود
drop policy if exists "exams_select_public" on public.exams;

-- جایگزین: دانشجو فقط آزمونِ سکشنی که در آن enrolled است را می‌بیند
create policy "exams_select_enrolled_student"
  on public.exams for select
  using (
    section_id in (
      select section_id from public.enrollments e
      join public.students s on s.student_code = e.student_code
      where s.auth_user_id = auth.uid() and e.status = 'enrolled'
    )
  );

-- استاد هم باید بتواند آزمون سکشن خودش را select کند (علاوه بر manage
-- بالا که for all است و select را هم پوشش می‌دهد، این تکراری نیست،
-- policy بالا کافیست)


-- ── ۷. اتصال exam_sessions به sections برای RLS دقیق‌تر تصحیح ──
-- exam_sessions از قبل از طریق exam_id -> exams -> section_id قابل
-- ردیابی است؛ policy های 0002 (professor_view_own_course/grade) باید
-- به section بروزرسانی شوند:
drop policy if exists "exam_sessions_professor_view_own_course" on public.exam_sessions;
drop policy if exists "exam_sessions_professor_grade_own_course" on public.exam_sessions;

create policy "exam_sessions_professor_view_own_section"
  on public.exam_sessions for select
  using (
    exam_id in (
      select exam_id from public.exams
      where section_id in (
        select section_id from public.sections
        where professor_id in (select id from public.professors where auth_user_id = auth.uid())
      )
    )
  );

create policy "exam_sessions_professor_grade_own_section"
  on public.exam_sessions for update
  using (
    exam_id in (
      select exam_id from public.exams
      where section_id in (
        select section_id from public.sections
        where professor_id in (select id from public.professors where auth_user_id = auth.uid())
      )
    )
  );


-- ── ۸. Data migration از ساختار قدیمی (courses/course_enrollments) ──
-- یک ترم پیش‌فرض برای داده‌های موجود می‌سازیم تا چیزی از دست نرود،
-- سپس هر courses قدیمی را به یک course_catalog + offering + section
-- تبدیل می‌کنیم (یک سکشن به ازای هر course قدیمی، همان استاد قبلی).
do $$
declare
  v_default_semester uuid;
  r_course record;
  v_catalog_id uuid;
  v_offering_id uuid;
  v_section_id uuid;
begin
  select semester_id into v_default_semester
  from public.semesters where title = 'ترم پیش‌فرض (مهاجرت‌شده)' limit 1;

  if v_default_semester is null then
    insert into public.semesters (title, starts_at, ends_at, is_active)
    values ('ترم پیش‌فرض (مهاجرت‌شده)', now() - interval '1 day', now() + interval '365 days', true)
    returning semester_id into v_default_semester;
  end if;

  for r_course in select * from public.courses loop
    -- کاتالوگ: یکی به ازای هر نام درس (در صورت تکراری بودن نام، جدا نگه‌داشته می‌شود
    -- چون course_code نداریم؛ ادمین می‌تواند بعداً یکی‌سازی کند)
    insert into public.course_catalog (course_title)
    values (r_course.course_name)
    returning course_id into v_catalog_id;

    insert into public.course_offerings (course_id, semester_id)
    values (v_catalog_id, v_default_semester)
    on conflict (course_id, semester_id) do nothing
    returning offering_id into v_offering_id;

    if v_offering_id is null then
      select offering_id into v_offering_id from public.course_offerings
      where course_id = v_catalog_id and semester_id = v_default_semester;
    end if;

    insert into public.sections (offering_id, professor_id, section_label)
    values (v_offering_id, r_course.professor_id, 'گروه ۱ (مهاجرت‌شده)')
    returning section_id into v_section_id;

    -- ربط examهای قدیمی همین course به section جدید
    update public.exams set section_id = v_section_id where course_id = r_course.course_id;

    -- ربط enrollment های قدیمی
    insert into public.enrollments (section_id, student_code, status)
    select v_section_id, ce.student_code, 'enrolled'
    from public.course_enrollments ce
    where ce.course_id = r_course.course_id
    on conflict (section_id, student_code) do nothing;
  end loop;
end $$;

-- توجه: بعد از تایید صحت داده‌ی مهاجرت‌شده و بروزرسانی کامل کد اپ به
-- section_id، در یک migration جدا می‌توان ستون‌ها/جداول قدیمی
-- (exams.course_id, courses, course_enrollments, policyهای مبتنی بر
-- course_id در 0002) را drop کرد. عمداً در همین migration حذف نشدند
-- تا در صورت باگ در کد جدید، امکان rollback سریع وجود داشته باشد.
