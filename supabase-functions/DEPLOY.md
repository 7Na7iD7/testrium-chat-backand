# استقرار Edge Functionهای چت

این دو تابع فقط از سمت سرور Rust (`chat-server`) صدا زده می‌شوند، نه
از اپ Flutter. چون فراخوانی با یک shared secret دلخواه انجام می‌شود
(نه با JWT معتبر یک کاربر لاگین‌کرده‌ی Supabase)، باید با
`--no-verify-jwt` مستقر شوند وگرنه gateway خودِ Supabase قبل از رسیدن
به کد تابع، درخواست را با ۴۰۱ رد می‌کند.

```bash
# یک‌بار، رمز مشترک را ست کن (باید دقیقاً با CHAT_SERVER_SHARED_SECRET
# در .env سرور Rust یکی باشد):
supabase secrets set CHAT_SERVER_SHARED_SECRET=<مقدار تصادفی امن>

supabase functions deploy chat-identity --no-verify-jwt
supabase functions deploy chat-verify-link --no-verify-jwt
```

## تست دستی بعد از deploy

```bash
curl -X POST https://xxxxx.supabase.co/functions/v1/chat-identity \
  -H "Authorization: Bearer <CHAT_SERVER_SHARED_SECRET>" \
  -H "Content-Type: application/json" \
  -d '{"auth_user_id": "<یک auth_user_id واقعی از جدول students>"}'

curl -X POST https://xxxxx.supabase.co/functions/v1/chat-verify-link \
  -H "Authorization: Bearer <CHAT_SERVER_SHARED_SECRET>" \
  -H "Content-Type: application/json" \
  -d '{"professor_id": "<uuid استاد>", "student_code": "<کد دانشجوی enrolled>"}'
```

پاسخ اول باید `{"ok": true, "role": "student", ...}` و دومی
`{"linked": true}` برگرداند.
