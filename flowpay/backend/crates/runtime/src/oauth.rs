use crate::{auth::{hash_secret, new_session_token}, error::ApiError, routes::{authenticate_dashboard, db}, state::AppState};
use axum::{extract::{Path, Query, State}, http::{header, HeaderMap, StatusCode}, response::{IntoResponse, Redirect, Response}, Form, Json};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use flowpay_domain::MerchantId;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use time::{Duration, OffsetDateTime};
use url::Url;
use uuid::Uuid;

const SCOPES: &[&str] = &["payments:read", "payments:write"];
pub fn issuer() -> String { std::env::var("FLOWPAY_OAUTH_ISSUER").unwrap_or_else(|_| "https://api.pixuno.xyz".into()).trim_end_matches('/').to_owned() }
pub fn resource() -> String { std::env::var("FLOWPAY_MCP_RESOURCE_URL").unwrap_or_else(|_| "https://mcp.pixuno.xyz/mcp".into()) }
fn invalid(message: &str) -> ApiError { ApiError::bad("invalid_request", message) }
fn invalid_grant() -> ApiError { ApiError::bad("invalid_grant", "authorization grant is invalid, expired, or already used") }
fn pkce_valid(verifier: &str, challenge: &str) -> bool {
    (43..=128).contains(&verifier.len()) && verifier.bytes().all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b)) && URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) == challenge
}
fn valid_redirect(value: &str) -> bool {
    let Ok(uri) = Url::parse(value) else { return false };
    uri.fragment().is_none() && uri.username().is_empty() && uri.password().is_none() && uri.host_str().is_some() && (uri.scheme() == "https" || (uri.scheme() == "http" && matches!(uri.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))))
}
pub async fn metadata() -> Json<Value> {
    let base = issuer();
    Json(json!({"issuer":base,"authorization_endpoint":format!("{base}/oauth/authorize"),"token_endpoint":format!("{base}/oauth/token"),"registration_endpoint":format!("{base}/oauth/register"),"revocation_endpoint":format!("{base}/oauth/revoke"),"response_types_supported":["code"],"grant_types_supported":["authorization_code","refresh_token"],"token_endpoint_auth_methods_supported":["none"],"code_challenge_methods_supported":["S256"],"scopes_supported":SCOPES,"authorization_response_iss_parameter_supported":true}))
}
#[derive(Deserialize)]
pub struct RegisterRequest { client_name: Option<String>, redirect_uris: Vec<String>, token_endpoint_auth_method: Option<String>, grant_types: Option<Vec<String>>, response_types: Option<Vec<String>> }
pub async fn register(State(state): State<AppState>, Json(req): Json<RegisterRequest>) -> Result<(StatusCode, Json<Value>), ApiError> {
    if req.redirect_uris.is_empty() || req.redirect_uris.len()>10 || req.redirect_uris.iter().any(|u| u.len()>2048 || !valid_redirect(u)) { return Err(invalid("register one to ten exact HTTPS redirect URIs")); }
    if req.token_endpoint_auth_method.as_deref().is_some_and(|m| m!="none") || req.grant_types.as_ref().is_some_and(|types| types.iter().any(|t| t!="authorization_code" && t!="refresh_token")) || req.response_types.as_ref().is_some_and(|types| types.iter().any(|t| t!="code")) { return Err(invalid("use a public client, authorization_code, refresh_token and code response type")); }
    let client_id = format!("fp_client_{}", new_session_token());
    let name = req.client_name.as_deref().unwrap_or("MCP client").trim().chars().take(120).collect::<String>();
    sqlx::query("INSERT INTO oauth_clients(client_id,client_name,redirect_uris) VALUES($1,$2,$3)").bind(&client_id).bind(&name).bind(&req.redirect_uris).execute(state.store.pool()).await.map_err(db)?;
    Ok((StatusCode::CREATED, Json(json!({"client_id":client_id,"client_name":name,"redirect_uris":req.redirect_uris,"token_endpoint_auth_method":"none","grant_types":["authorization_code","refresh_token"],"response_types":["code"]}))))
}
#[derive(Deserialize)]
pub struct AuthorizeRequest { client_id: String, redirect_uri: String, response_type: String, code_challenge: String, code_challenge_method: String, resource: String, scope: Option<String>, state: Option<String> }
pub async fn authorize(State(state): State<AppState>, Query(req): Query<AuthorizeRequest>) -> Result<Redirect, ApiError> {
    let redirects: Option<Vec<String>> = sqlx::query_scalar("SELECT redirect_uris FROM oauth_clients WHERE client_id=$1").bind(&req.client_id).fetch_optional(state.store.pool()).await.map_err(db)?;
    if !redirects.is_some_and(|uris| uris.contains(&req.redirect_uri)) { return Err(invalid("unregistered client or redirect URI")); }
    if req.response_type!="code" || req.code_challenge_method!="S256" || req.code_challenge.len()!=43 || !req.code_challenge.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)) || req.resource!=resource() || req.state.as_ref().is_some_and(|s| s.len()>2048) { return Err(invalid("use code, PKCE S256, and the advertised MCP resource")); }
    let scopes = req.scope.as_deref().unwrap_or("payments:read payments:write").split_whitespace().map(str::to_owned).collect::<Vec<_>>();
    if scopes.is_empty() || scopes.iter().any(|s| !SCOPES.contains(&s.as_str())) { return Err(ApiError::bad("invalid_scope", "unsupported OAuth scope")); }
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO oauth_requests(id,client_id,redirect_uri,resource,scopes,state,code_challenge,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(id).bind(req.client_id).bind(req.redirect_uri).bind(req.resource).bind(scopes).bind(req.state).bind(req.code_challenge).bind(OffsetDateTime::now_utc()+Duration::minutes(10)).execute(state.store.pool()).await.map_err(db)?;
    let consent_base=std::env::var("FLOWPAY_OAUTH_CONSENT_BASE_URL").unwrap_or_else(|_| state.config.dashboard_base_url.clone());
    Ok(Redirect::to(&format!("{}/oauth/authorize?request={id}", consent_base.trim_end_matches('/'))))
}
pub async fn request_details(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<Uuid>) -> Result<Json<Value>, ApiError> {
    let _ = authenticate_dashboard(&state, &headers).await?;
    let row = sqlx::query("SELECT r.redirect_uri,r.scopes,c.client_name FROM oauth_requests r JOIN oauth_clients c USING(client_id) WHERE r.id=$1 AND r.status='PENDING' AND r.expires_at>now()").bind(id).fetch_optional(state.store.pool()).await.map_err(db)?.ok_or_else(invalid_grant)?;
    Ok(Json(json!({"id":id,"client_name":row.try_get::<String,_>("client_name").map_err(db)?,"redirect_uri":row.try_get::<String,_>("redirect_uri").map_err(db)?,"scopes":row.try_get::<Vec<String>,_>("scopes").map_err(db)?})))
}
#[derive(Deserialize)]
pub struct ConsentRequest { approved: bool }
pub async fn consent(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<Uuid>, Json(req): Json<ConsentRequest>) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate_dashboard(&state, &headers).await?;
    let mut tx = state.store.pool().begin().await.map_err(db)?;
    let row = sqlx::query("SELECT * FROM oauth_requests WHERE id=$1 AND status='PENDING' AND expires_at>now() FOR UPDATE").bind(id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(invalid_grant)?;
    let mut redirect = Url::parse(&row.try_get::<String,_>("redirect_uri").map_err(db)?).map_err(|_| invalid("invalid redirect"))?;
    if let Some(state_value) = row.try_get::<Option<String>,_>("state").map_err(db)? { redirect.query_pairs_mut().append_pair("state",&state_value); }
    redirect.query_pairs_mut().append_pair("iss",&issuer());
    if req.approved {
        let connection_id = Uuid::now_v7();
        let code = new_session_token();
        sqlx::query("INSERT INTO oauth_connections(id,merchant_id,client_id,resource,scopes) VALUES($1,$2,$3,$4,$5)").bind(connection_id).bind(merchant.0).bind(row.try_get::<String,_>("client_id").map_err(db)?).bind(row.try_get::<String,_>("resource").map_err(db)?).bind(row.try_get::<Vec<String>,_>("scopes").map_err(db)?).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE oauth_requests SET status='APPROVED',connection_id=$2,code_hash=$3,expires_at=now()+interval '5 minutes' WHERE id=$1").bind(id).bind(connection_id).bind(hash_secret(&code)).execute(&mut *tx).await.map_err(db)?;
        redirect.query_pairs_mut().append_pair("code",&code);
    } else {
        sqlx::query("UPDATE oauth_requests SET status='DENIED' WHERE id=$1").bind(id).execute(&mut *tx).await.map_err(db)?;
        redirect.query_pairs_mut().append_pair("error","access_denied");
    }
    tx.commit().await.map_err(db)?;
    Ok(Json(json!({"redirect_uri":redirect.as_str()})))
}
#[derive(Deserialize)]
pub struct TokenRequest { grant_type: String, client_id: String, resource: String, code: Option<String>, code_verifier: Option<String>, redirect_uri: Option<String>, refresh_token: Option<String> }
async fn issue_tokens(tx: &mut Transaction<'_, Postgres>, connection: Uuid, scopes: &[String]) -> Result<Value, ApiError> {
    let access = format!("fp_oauth_{}",new_session_token());
    let refresh = format!("fp_refresh_{}",new_session_token());
    for (token,kind,ttl) in [(&access,"ACCESS",Duration::hours(1)),(&refresh,"REFRESH",Duration::days(30))] {
        sqlx::query("INSERT INTO oauth_tokens(token_hash,connection_id,kind,issuer,expires_at) VALUES($1,$2,$3,$4,$5)").bind(hash_secret(token)).bind(connection).bind(kind).bind(issuer()).bind(OffsetDateTime::now_utc()+ttl).execute(&mut **tx).await.map_err(db)?;
    }
    Ok(json!({"access_token":access,"token_type":"Bearer","expires_in":3600,"refresh_token":refresh,"scope":scopes.join(" "),"resource":resource()}))
}
pub async fn token(State(state): State<AppState>, Form(req): Form<TokenRequest>) -> Response {
    match exchange(&state,req).await {
        Ok(value) => ([(header::CACHE_CONTROL,"no-store"),(header::PRAGMA,"no-cache")],Json(value)).into_response(),
        Err(error) => (error.status,Json(json!({"error":error.code,"error_description":error.message}))).into_response(),
    }
}
async fn exchange(state: &AppState, req: TokenRequest) -> Result<Value,ApiError> {
    if req.resource!=resource() { return Err(ApiError::bad("invalid_target","wrong resource audience")); }
    let mut tx = state.store.pool().begin().await.map_err(db)?;
    let (connection,scopes) = match req.grant_type.as_str() {
        "authorization_code" => {
            let code = req.code.as_deref().ok_or_else(invalid_grant)?;
            let row = sqlx::query("SELECT r.*,c.revoked_at,m.status AS merchant_status FROM oauth_requests r JOIN oauth_connections c ON c.id=r.connection_id JOIN merchants m ON m.id=c.merchant_id WHERE r.code_hash=$1 AND r.status='APPROVED' AND r.expires_at>now() FOR UPDATE OF r").bind(hash_secret(code)).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(invalid_grant)?;
            if row.try_get::<String,_>("client_id").map_err(db)?!=req.client_id || Some(row.try_get::<String,_>("redirect_uri").map_err(db)?)!=req.redirect_uri || row.try_get::<String,_>("resource").map_err(db)?!=req.resource || row.try_get::<Option<OffsetDateTime>,_>("revoked_at").map_err(db)?.is_some() || row.try_get::<String,_>("merchant_status").map_err(db)?!="ACTIVE" || !pkce_valid(req.code_verifier.as_deref().unwrap_or(""),&row.try_get::<String,_>("code_challenge").map_err(db)?) { return Err(invalid_grant()); }
            sqlx::query("UPDATE oauth_requests SET status='CONSUMED' WHERE id=$1").bind(row.try_get::<Uuid,_>("id").map_err(db)?).execute(&mut *tx).await.map_err(db)?;
            (row.try_get::<Uuid,_>("connection_id").map_err(db)?,row.try_get::<Vec<String>,_>("scopes").map_err(db)?)
        },
        "refresh_token" => {
            let row = sqlx::query("SELECT t.token_hash,t.consumed_at,t.connection_id,c.client_id,c.resource,c.scopes FROM oauth_tokens t JOIN oauth_connections c ON c.id=t.connection_id JOIN merchants m ON m.id=c.merchant_id WHERE t.token_hash=$1 AND t.kind='REFRESH' AND t.expires_at>now() AND t.issuer=$2 AND c.revoked_at IS NULL AND m.status='ACTIVE' FOR UPDATE OF t,c").bind(hash_secret(req.refresh_token.as_deref().ok_or_else(invalid_grant)?)).bind(issuer()).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(invalid_grant)?;
            if row.try_get::<String,_>("client_id").map_err(db)?!=req.client_id || row.try_get::<String,_>("resource").map_err(db)?!=req.resource { return Err(invalid_grant()); }
            let connection = row.try_get::<Uuid,_>("connection_id").map_err(db)?;
            if row.try_get::<Option<OffsetDateTime>,_>("consumed_at").map_err(db)?.is_some() {
                sqlx::query("UPDATE oauth_connections SET revoked_at=now() WHERE id=$1").bind(connection).execute(&mut *tx).await.map_err(db)?;
                tx.commit().await.map_err(db)?;
                return Err(invalid_grant());
            }
            sqlx::query("UPDATE oauth_tokens SET consumed_at=now() WHERE token_hash=$1").bind(row.try_get::<String,_>("token_hash").map_err(db)?).execute(&mut *tx).await.map_err(db)?;
            (connection,row.try_get::<Vec<String>,_>("scopes").map_err(db)?)
        },
        _ => return Err(ApiError::bad("unsupported_grant_type","use authorization_code or refresh_token")),
    };
    let value = issue_tokens(&mut tx,connection,&scopes).await?;
    tx.commit().await.map_err(db)?;
    Ok(value)
}
pub async fn validate_access(state: &AppState, token: &str, required_scope: Option<&str>) -> Result<(MerchantId,Vec<String>),ApiError> {
    let row = sqlx::query("SELECT c.merchant_id,c.scopes FROM oauth_tokens t JOIN oauth_connections c ON c.id=t.connection_id JOIN merchants m ON m.id=c.merchant_id WHERE t.token_hash=$1 AND t.kind='ACCESS' AND t.expires_at>now() AND t.issuer=$2 AND t.consumed_at IS NULL AND c.resource=$3 AND c.revoked_at IS NULL AND m.status='ACTIVE'").bind(hash_secret(token)).bind(issuer()).bind(resource()).fetch_optional(state.store.pool()).await.map_err(db)?.ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED,"invalid_token","OAuth token is invalid, expired, revoked, or has the wrong audience"))?;
    let scopes = row.try_get::<Vec<String>,_>("scopes").map_err(db)?;
    if required_scope.is_some_and(|s| !scopes.iter().any(|v| v==s)) { return Err(ApiError::new(StatusCode::FORBIDDEN,"insufficient_scope","OAuth scope does not permit this operation")); }
    Ok((MerchantId(row.try_get::<Uuid,_>("merchant_id").map_err(db)?),scopes))
}
pub async fn token_info(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Value>,ApiError> {
    let token = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).ok_or_else(invalid_grant)?;
    let (merchant,scopes) = validate_access(&state,token,None).await?;
    Ok(Json(json!({"active":true,"merchant_id":merchant.0,"scope":scopes.join(" "),"issuer":issuer(),"audience":resource()})))
}
#[derive(Deserialize)]
pub struct RevokeRequest { token: String, client_id: String }
pub async fn revoke(State(state): State<AppState>, Form(req): Form<RevokeRequest>) -> Result<StatusCode,ApiError> {
    sqlx::query("UPDATE oauth_connections SET revoked_at=now() WHERE client_id=$1 AND id IN (SELECT connection_id FROM oauth_tokens WHERE token_hash=$2)").bind(req.client_id).bind(hash_secret(&req.token)).execute(state.store.pool()).await.map_err(db)?;
    Ok(StatusCode::OK)
}
pub async fn connections(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Value>,ApiError> {
    let merchant = authenticate_dashboard(&state,&headers).await?;
    let rows = sqlx::query("SELECT c.id,c.scopes,c.created_at,c.revoked_at,p.client_name FROM oauth_connections c JOIN oauth_clients p USING(client_id) WHERE c.merchant_id=$1 ORDER BY c.created_at DESC").bind(merchant.0).fetch_all(state.store.pool()).await.map_err(db)?;
    let data = rows.iter().map(|r| Ok(json!({"id":r.try_get::<Uuid,_>("id").map_err(db)?,"client_name":r.try_get::<String,_>("client_name").map_err(db)?,"scopes":r.try_get::<Vec<String>,_>("scopes").map_err(db)?,"created_at":r.try_get::<OffsetDateTime,_>("created_at").map_err(db)?.unix_timestamp()*1000,"revoked":r.try_get::<Option<OffsetDateTime>,_>("revoked_at").map_err(db)?.is_some()}))).collect::<Result<Vec<Value>,ApiError>>()?;
    Ok(Json(json!({"data":data})))
}
pub async fn disconnect(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<Uuid>) -> Result<StatusCode,ApiError> {
    let merchant = authenticate_dashboard(&state,&headers).await?;
    sqlx::query("UPDATE oauth_connections SET revoked_at=now() WHERE id=$1 AND merchant_id=$2").bind(id).bind(merchant.0).execute(state.store.pool()).await.map_err(db)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn pkce_requires_the_exact_verifier() {
        let verifier="dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert!(pkce_valid(verifier,"E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"));
        assert!(!pkce_valid("wrong","E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"));
    }
    #[test] fn redirect_validation_blocks_credentials_and_fragments() {
        assert!(valid_redirect("https://chatgpt.com/connector_platform_oauth_redirect"));
        assert!(valid_redirect("http://127.0.0.1:1234/callback"));
        assert!(!valid_redirect("https://user:secret@example.com/callback"));
        assert!(!valid_redirect("https://example.com/callback#fragment"));
        assert!(!valid_redirect("http://example.com/callback"));
        assert!(!valid_redirect("javascript:alert(1)"));
    }
}
