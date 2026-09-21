# مانیتورینگ سرور ۱ (Supabase self-host) — گام‌به‌گام

این راهنما فرض می‌کنه سرور ۱ از قبل طبق `README.md`/`server-request-checklist.md`
راه‌اندازی شده (یعنی `~/testrium-supabase` با `docker-compose.yml` رسمی
Supabase از قبل بالا هست). اینجا فقط مانیتورینگش رو اضافه می‌کنیم.

---

## قدم ۱ — فایل override رو بذار

فایل `docker-compose.override.yml` رو دقیقاً کنار `docker-compose.yml` اصلی
Supabase بذار:

```
~/testrium-supabase/docker-compose.override.yml
```

Docker Compose به‌صورت خودکار این فایل رو با اصلی merge می‌کنه — نیازی به
پرچم `-f` اضافه نیست، فقط با همون دستور همیشگی:

```bash
cd ~/testrium-supabase
docker compose up -d
```

**چک:**
```bash
docker compose ps | grep pg-exporter
```
باید `supabase-pg-exporter` رو با وضعیت `Up` ببینی.

```bash
curl http://localhost:9187/metrics | head -10
```
باید خروجی متنی با خط‌هایی شبیه `pg_up` ببینی.

---

## قدم ۲ — فایروال رو محدود کن

طبق `FIREWALL_SERVER1.md`، فقط IP سرور ۲ اجازه‌ی رسیدن به پورت `9187` رو
داشته باشه. این قدم رو رد نکن — وگرنه متریک داخلی Postgres روی اینترنت باز
می‌مونه.

---

## قدم ۳ — سرور ۲ رو بهش وصل کن

روی **سرور ۲** (جایی که Prometheus واقعی اجرا می‌شه)، فایل
`monitoring/prometheus.yml` رو باز کن و به‌جای `<IP-سرور-۱>` آدرس واقعی سرور
۱ رو بذار (این خط از قبل توی فایل باز شده، فقط IP رو جایگزین کن):

```yaml
  - job_name: 'supabase-postgres'
    static_configs:
      - targets: ['<IP-سرور-۱>:9187']
```

بعد Prometheus رو ری‌استارت کن تا کانفیگ جدید رو بخونه:
```bash
cd ~/testrium-chat
docker compose restart prometheus
```

---

## قدم ۴ — تایید نهایی

توی مرورگر برو به:
```
http://<IP-سرور-۲>:9091/targets
```
(یا از پشت Nginx اگه تنظیمش کردی)

باید سطر `supabase-postgres` رو با وضعیت سبز `UP` ببینی. اگه قرمز بود یا
`DOWN` بود، یعنی یا فایروال هنوز مسدودش کرده یا IP اشتباهه — این دو تا رو
اول چک کن.

بعد از این، داشبورد Postgres (شناسه‌ی `9628` که توی Grafana Import کردیم)
دیتای سرور ۱ رو هم نشون می‌ده — کار اضافه‌ای لازم نیست.
