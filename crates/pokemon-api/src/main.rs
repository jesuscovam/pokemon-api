use aws_lambda_events::{
    encodings::Body,
    event::apigw::{
        ApiGatewayProxyRequest, ApiGatewayProxyResponse
    }
};
use http::header::HeaderMap;
use lambda_runtime::{handler_fn, Context, Error};
use serde::Serialize;
use sqlx::{mysql::MySqlPoolOptions, Pool, MySql};
use serde_json::json;
use tracing_subscriber;
use tracing::{error, info, instrument};
use once_cell::sync::OnceCell;

static POOL: OnceCell<Pool<MySql>> = OnceCell::new();


#[derive(Debug, sqlx::FromRow, Serialize)]
struct PokemonHp {
    name: String,
    hp: u16,
    legendary_or_mythical: bool
}

#[tokio::main]
async fn main() -> Result<(), Error>{
    tracing_subscriber::fmt::init();
    setup_db_connection("DATABASE_URL".to_string()).await;
    let processor = handler_fn(handler);
    lambda_runtime::run(processor).await?;
    Ok(())
}

#[instrument]
async fn handler(	
    event: ApiGatewayProxyRequest,	
    _: Context,	
) -> Result<ApiGatewayProxyResponse, Error> {	
    println!("handler");	
    let path = event.path.expect("expect there to always be a path");
    let requested_pokemon = 
        path
        .split("/")
        .last();

    match requested_pokemon {
        Some("") => {
            error!("searched for empty pokemon");
            let error_message = serde_json::to_string(&json!({
                "error": "searched for empty pokemon"
            }))?;

            let response = ApiGatewayProxyResponse {
                status_code: 400,
                headers: HeaderMap::new(),
                multi_value_headers: HeaderMap::new(),
                body: Some(Body::Text(error_message)),
                is_base64_encoded: Some(false)
            };
            Ok(response)
        },
        None => panic!("requested_pokemon is None, which should never happen"),
        Some(pokemon_name) => {
            info!(pokemon_name,"requested a pokemon");

            let result = sqlx::query_as!(
                PokemonHp,
                r#"SELECT name, hp, legendary_or_mythical as "legendary_or_mythical!: bool" FROM pokemon WHERE slug = ?"#,
                pokemon_name
            ).fetch_one(POOL.get().unwrap())
            .await?;
        
            let json_pokemon = serde_json::to_string(&result)?;
            
            let response = ApiGatewayProxyResponse {	
                status_code: 200,	
                headers: HeaderMap::new(),	
                multi_value_headers: HeaderMap::new(),	
                body: Some(Body::Text(json_pokemon)),	
                is_base64_encoded: Some(false),	
            };	
            Ok(response)	
        }
    }
}


async fn setup_db_connection(db_url: String) {
    let database_url = std::env::var(db_url).unwrap();

    let pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .unwrap();

    POOL.get_or_init(|| pool);
    
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use aws_lambda_events::event::apigw::{
        ApiGatewayProxyRequestContext,
        ApiGatewayRequestIdentity,
    };
    use http::Method;
    use super::*;

    fn fake_request(
        path: String,
    ) -> ApiGatewayProxyRequest {
        ApiGatewayProxyRequest {
            resource: None,
            path: Some(path),
            http_method: Method::GET,
            headers: HeaderMap::new(),
            multi_value_headers: HeaderMap::new(),
            query_string_parameters: HashMap::new(),
            multi_value_query_string_parameters:
                HashMap::new(),
            path_parameters: HashMap::new(),
            stage_variables: HashMap::new(),
            request_context:
                ApiGatewayProxyRequestContext {
                    account_id: None,
                    resource_id: None,
                    operation_name: None,
                    stage: None,
                    domain_name: None,
                    domain_prefix: None,
                    request_id: None,
                    protocol: None,
                    identity: ApiGatewayRequestIdentity {
                        cognito_identity_pool_id: None,
                        account_id: None,
                        cognito_identity_id: None,
                        caller: None,
                        api_key: None,
                        api_key_id: None,
                        access_key: None,
                        source_ip: None,
                        cognito_authentication_type: None,
                        cognito_authentication_provider:
                            None,
                        user_arn: None,
                        user_agent: None,
                        user: None,
                    },
                    resource_path: None,
                    authorizer: HashMap::new(),
                    http_method: Method::GET,
                    request_time: None,
                    request_time_epoch: 0,
                    apiid: None,
                },
            body: None,
            is_base64_encoded: Some(false),
        }
    }

    #[tokio::test]
    async fn handler_handles() {
        let event = fake_request("/api/pokemon/bulbasaur".to_string());
        setup_db_connection("DATABASE_URL".to_string()).await;
        assert_eq!(
            handler(event.clone(), Context::default())
                .await
                .unwrap(),
            ApiGatewayProxyResponse {
                status_code: 200,
                headers: HeaderMap::new(),
                multi_value_headers: HeaderMap::new(),
                body: Some(Body::Text(serde_json::to_string(&PokemonHp {
                    name: String::from("Bulbasaur"),
                    hp: 45,
                    legendary_or_mythical: false
                }).unwrap())),
                is_base64_encoded: Some(false)
            },
        )
    }

    #[tokio::test]
    async fn handler_handles_empty_pokemon() {
        let event = fake_request("/api/pokemon//".to_string());
        let response = ApiGatewayProxyResponse {
            status_code: 400,
            headers: HeaderMap::new(),
            multi_value_headers: HeaderMap::new(),
            body: Some(Body::Text(
                serde_json::to_string(&json!({ "error": "searched for empty pokemon"})).unwrap()
            )),
            is_base64_encoded: Some(false)
        };

        assert_eq!(
            handler(event.clone(), Context::default()).await.unwrap(),
            response
        );
    }
}
