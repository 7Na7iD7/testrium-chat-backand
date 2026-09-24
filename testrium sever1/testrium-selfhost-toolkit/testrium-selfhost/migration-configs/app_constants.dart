class AppConstants {
  AppConstants._();

  // TODO(migration): بعد از بالا آمدن instance self-hosted، این دو مقدار
  // رو با خروجی واقعی جایگزین کن:
  //   - supabaseUrl        -> API_EXTERNAL_URL از .env instance جدید
  //                            (همون چیزی که در Nginx روی supabase.<دامنه دانشگاه> ست کردی)
  //   - supabaseAnonKey    -> ANON_KEY تولیدشده با scripts/00_generate_jwt_keys.py
  static const String supabaseUrl = 'https://supabase.YOUR-UNIVERSITY-DOMAIN';
  static const String supabaseAnonKey = 'REPLACE_WITH_NEW_ANON_KEY';

  static const String sentryDsn = String.fromEnvironment(
    'SENTRY_DSN',
    defaultValue:
    'https://fe0b5304d41f686047efc89f0857f5d2@o4511742797676544.ingest.us.sentry.io/4511742803050501',
  );
  static const String posthogApiKey = String.fromEnvironment(
    'POSTHOG_API_KEY',
    defaultValue: 'phc_B6CtvPsCKx3fa97vRpFoJjJt6o5PDJdc4wZUCpp49r2b',
  );
  static const String posthogHost = String.fromEnvironment(
    'POSTHOG_HOST',
    defaultValue: 'https://us.i.posthog.com',
  );

  static const String examContentBucket = 'exam-content';

  static const String verifyCodeFn = 'verify-code';
  static const String submitExamResultFn = 'submit-exam-result';

  static const Duration fetchTimeout = Duration(seconds: 10);
  static const int maxRetries = 3;

  static const Duration minRevalidateInterval = Duration(seconds: 20);
  static const Duration maxBackoff = Duration(minutes: 5);
  static const int failureThresholdForBackoff = 2;
  static const int examMemoryCacheEntries = 20;
  static const int indexMemoryCacheEntries = 2;

  static const int minExamDurationMinutes = 1;
  static const int maxExamDurationMinutes = 300;
  static const double defaultMinutesPerMcq = 2.0;
  static const double defaultMinutesPerEssay = 4.0;

  static const String themeModeKey = 'testrium_theme_mode';
  static const String examSessionPrefix = 'exam_session_';
  static const String appLoggedOutKey = 'testrium_app_logged_out';
  static const String hasSeenWelcomeKey = 'testrium_has_seen_welcome';

  static const int studentCodeMinLength = 4;
  static const int studentCodeMaxLength = 15;
}