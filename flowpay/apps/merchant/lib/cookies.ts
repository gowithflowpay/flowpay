// Shared by middleware (edge runtime) and server code, so it must stay free of
// Node-only imports.
export const SESSION_COOKIE = "flowpay_session";

// Mirrors the session's onboarding state so middleware can gate the dashboard
// without an API round trip on every request.
export const ONBOARDED_COOKIE = "flowpay_onboarded";

export const SESSION_MAX_AGE = 60 * 60 * 24 * 14;
