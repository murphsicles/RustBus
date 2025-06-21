use actix_web::{HttpResponse, web, get, post};
use async_graphql_actix_web::{GraphQLRequest, GraphQLResponse};
use async_graphql::http::playground_source;
use super::AppState;

#[post("/graphql")]
pub async fn graphql(state: web::Data<AppState>, request: GraphQLRequest) -> GraphQLResponse {
    state.schema.execute(request.into_inner()).await.into()
}

#[get("/graphql")]
pub async fn graphiql() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(playground_source("/graphql"))
}
