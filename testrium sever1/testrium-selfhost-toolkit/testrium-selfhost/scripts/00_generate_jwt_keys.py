#!/usr/bin/env python3
"""
تولید ANON_KEY و SERVICE_ROLE_KEY برای Supabase self-hosted.
اجرا: python3 00_generate_jwt_keys.py <JWT_SECRET>
نیازمند: pip install pyjwt --break-system-packages
"""
import sys
import time
import jwt  # PyJWT

if len(sys.argv) != 2:
    print("استفاده: python3 00_generate_jwt_keys.py <JWT_SECRET>")
    sys.exit(1)

secret = sys.argv[1]
now = int(time.time())
exp = now + 60 * 60 * 24 * 365 * 10  # ۱۰ سال اعتبار (کلید API، نه توکن کاربر)

for role in ("anon", "service_role"):
    payload = {
        "role": role,
        "iss": "supabase",
        "iat": now,
        "exp": exp,
    }
    token = jwt.encode(payload, secret, algorithm="HS256")
    print(f"{role.upper()}_KEY={token}")
