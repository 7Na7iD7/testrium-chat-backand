# دستورات اتصال فلاتر + chat-infra به Supabase Self-Hosted

## پیش‌نیاز

قبل از هر چیز باید instance self-hosted کامل بالا باشه و این دو مقدار رو در دست داشته باشی:
- `API_EXTERNAL_URL` (مثلاً `https://supabase.دانشگاه.ir`)
- `ANON_KEY` و `SERVICE_ROLE_KEY` (از `scripts/00_generate_jwt_keys.py`)

⚠️ نکته‌ی امنیتی: `CHAT_SERVER_SHARED_SECRET` فعلی (`my-dev-secret-12345`) یک secret توسعه‌ست، نه production. توصیه می‌کنم قبل از دیپلوی واقعی با `openssl rand -base64 32` یک مقدار جدید بسازی و همزمان در **سه جا** (Rust `.env`، Edge Function `chat-identity`، Edge Function `chat-verify-link`) یکسان جایگزینش کنی.

---

## ۱) فلاتر — `app_constants.dart`

فایل اصلاح‌شده در `migration-configs/app_constants.dart` رو جایگزین فایل اصلی کن:

```bash
# روی سیستم توسعه (ویندوز، طبق مسیر پروژه‌ت)
cp migration-configs/app_constants.dart <PROJECT_ROOT>/lib/core/constants/app_constants.dart
```

سپس داخل فایل، این دو خط رو با مقادیر واقعی جایگزین کن:
```dart
static const String supabaseUrl = 'https://supabase.YOUR-UNIVERSITY-DOMAIN';   // -> آدرس واقعی
static const String supabaseAnonKey = 'REPLACE_WITH_NEW_ANON_KEY';              // -> ANON_KEY واقعی
```

### rebuild و انتشار

چون این تغییر روی یک constant استاتیک Dart هست (نه asset/native)، دقیقاً واجد شرایط **Shorebird patch** هست (بخش ۱۹ گزارش پروژه‌ت) — نیازی به release کامل جدید در استور نیست:

```bash
cd <PROJECT_ROOT>

# تست محلی اول (مطمئن شو اپ به instance جدید وصل می‌شه، لاگین کار می‌کنه)
flutter run

# اگر تست موفق بود، پچ رو برای کاربران فعلی ارسال کن
shorebird patch android   # یا ios، بسته به پلتفرمی که منتشر کردی
```

اگر می‌خوای همزمان یک build کامل جدید هم داشته باشی (مثلاً برای نصب‌های تازه):
```bash
shorebird release android
```

---

## ۲) بک‌اند چت (Rust) — `.env`

فایل اصلاح‌شده `migration-configs/chat-server.env` رو جایگزین `.env` واقعی کن:

```bash
# روی سروری که chat-infra اجرا می‌شه
cp migration-configs/chat-server.env ~/testrium-chat/.env
nano ~/testrium-chat/.env   # مقادیر REPLACE_WITH_* رو با مقادیر واقعی پر کن
```

### rebuild و restart

```bash
cd ~/testrium-chat

# اگر با systemd اجرا می‌شه:
cargo build --release
sudo systemctl restart testrium-chat
sudo systemctl status testrium-chat
journalctl -u testrium-chat -f   # چک کردن لاگ زنده، باید بدون خطای JWT/JWKS بالا بیاد

# اگر با docker-compose اجرا می‌شه:
docker compose up -d --build
docker compose logs -f chat-infra
```

---

## ۳) تست end-to-end بعد از هر دو تغییر

با یک اکانت تستی (نه واقعی):

1. **ورود دانشجو/استاد**: از اپ فلاتر با کد ورود واقعی لاگین کن → باید `verify-code` روی instance جدید موفق باشه
2. **submit آزمون**: یک آزمون تستی رو ثبت کن → نمره باید درست محاسبه بشه (یعنی `submit-exam-result` هم روی instance جدید کار می‌کنه)
3. **چت زنده**: از پنل استاد یک پیام به دانشجو بفرست → باید فوری در پنل دانشجو ظاهر بشه (یعنی `chat-infra` تونسته JWT دانشجو رو با مسیر HS256/legacy_jwt_secret verify کنه و `chat-identity`/`chat-verify-link` رو موفق صدا زده)

اگر مرحله‌ی ۳ با خطای احراز هویت (`401`/`invalid signature`) شکست خورد، محتمل‌ترین علت اینه که `SUPABASE_JWT_SECRET` در `.env` سرویس چت با `JWT_SECRET` واقعی instance self-hosted یکی نیست (بخش ۱۰ راهنمای کامل، `docs/full-guide.md`) — این دو مقدار باید حرف‌به‌حرف یکسان باشن.

---

## چک‌لیست نهایی

- [ ] `app_constants.dart` با URL/anon key جدید rebuild و `shorebird patch` شده
- [ ] `.env` سرویس چت با مقادیر جدید و `chat-infra` restart شده
- [ ] `CHAT_SERVER_SHARED_SECRET` جدید (نه dev secret قدیمی) در هر سه محل یکسانه
- [ ] لاگین دانشجو/استاد تست شده
- [ ] submit آزمون تست شده
- [ ] پیام چت زنده تست شده
