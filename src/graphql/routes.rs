use actix_web::{HttpResponse, web, get, post};
use async_graphql_actix_web::{GraphQLRequest, GraphQLResponse};
use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use super::AppState;

// Handles POST /graphql requests for executing GraphQL queries and mutations
#[post("/graphql")]
pub async fn graphql(state: web::Data<AppState>, request: GraphQLRequest) -> GraphQLResponse {
    // Execute the GraphQL request using the schema stored in the application state
    state.schema.execute(request.into_inner()).await.into()
}

// Handles GET /graphql requests to serve the GraphiQL UI
#[get("/graphql")]
pub async fn graphiql() -> HttpResponse {
    // Return an HTML response with the GraphiQL interface, configured to query the /graphql endpoint
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(playground_source(GraphQLPlaygroundConfig::new("/graphql")))
}
