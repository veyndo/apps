//! Scoped merchant-only evidence retrieval; sources never enter public product answers.
use super::*;
pub(crate) fn router() -> Router<App> {
    Router::new().secure_route(
        "/api/knowledge/external",
        &[("GET", "knowledge.read")],
        get(list),
    )
}
async fn list(
    State(a): State<App>,
    h: RequestContext,
    axum::extract::Query(v): axum::extract::Query<HashMap<String, String>>,
) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    auth::permit(&h, "knowledge.read")?;
    Ok(Json(
        json!({"elements":private_evidence(&a,&t,v.get("query").map(String::as_str).unwrap_or("")).await?,"graph":super::evidence::private_graph(&a,&t).await?,"visibility":"merchant-private"}),
    ))
}
