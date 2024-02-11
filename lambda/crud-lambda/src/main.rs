#![feature(lazy_cell)]

use std::{collections::HashMap, env::set_var, sync::LazyLock};

use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::{types::AttributeValue, Client};
use axum::{
    extract::Path,
    http::{Method, StatusCode},
    routing::get,
    Json, Router,
};
use lambda_http::{run, Error};
use serde_dynamo::aws_sdk_dynamodb_1::{from_item, from_items, to_attribute_value, to_item};
use serde_json::Value;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::filter::{EnvFilter, LevelFilter};

static TABLE_NAME: LazyLock<String> =
    LazyLock::new(|| std::env::var("TABLE_NAME").expect("TABLE_NAME must be set"));
static PK: LazyLock<String> = LazyLock::new(|| std::env::var("PK").expect("PK must be set"));

async fn dynamo() -> Client {
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    aws_sdk_dynamodb::Client::new(&config)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    set_var("AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH", "true");

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with_target(false)
        .without_time()
        .init();

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::PATCH])
        .allow_origin(Any);

    let app = Router::new()
        .route("/items", get(get_all).post(create))
        .route("/:id", get(get_one).delete(delete_one).patch(update_one))
        .layer(cors);

    run(app).await
}

async fn create(Json(mut body): Json<Value>) -> Result<(), StatusCode> {
    let client = dynamo().await;

    body.as_object_mut()
        .expect("body must be an object")
        .insert(
            PK.to_string(),
            Value::String(uuid::Uuid::new_v4().to_string()),
        );

    let _ = client
        .put_item()
        .table_name(TABLE_NAME.to_string())
        .set_item(to_item(body).ok())
        .send()
        .await
        .map_err(|e| {
            tracing::error!("error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(())
}

async fn get_one(Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let client = dynamo().await;
    let item = client
        .get_item()
        .table_name(TABLE_NAME.to_string())
        .key(PK.to_string(), AttributeValue::S(id))
        .send()
        .await
        .map_err(|e| {
            tracing::error!("error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .item
        .expect("item not found");

    Ok(Json(from_item(item).ok().unwrap()))
}

async fn get_all() -> Result<Json<Vec<Value>>, StatusCode> {
    let client = dynamo().await;
    let items = client
        .scan()
        .table_name(TABLE_NAME.to_string())
        .send()
        .await
        .map_err(|e| {
            tracing::error!("error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .items
        .expect("items not found");

    Ok(Json(from_items(items).ok().unwrap()))
}

async fn delete_one(Path(id): Path<String>) -> Result<(), StatusCode> {
    let client = dynamo().await;
    let _ = client
        .delete_item()
        .table_name(TABLE_NAME.to_string())
        .key(PK.to_string(), AttributeValue::S(id))
        .send()
        .await
        .map_err(|e| {
            tracing::error!("error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(())
}

async fn update_one(Path(id): Path<String>, Json(body): Json<Value>) -> Result<(), StatusCode> {
    let client = dynamo().await;

    let (update, remove, expression_attribute_name, expression_attribute_value) = body
        .as_object()
        .expect("body must be an object")
        .iter()
        .fold(
            (vec![], vec![], HashMap::new(), HashMap::new()),
            |(
                mut update,
                mut remove,
                mut expression_attribute_name,
                mut expression_attribute_value,
            ),
             (k, v)| {
                if v.is_null() {
                    remove.push(format!("#{}", k));
                } else {
                    update.push(format!("#{} = :{}", k, k));
                    expression_attribute_value
                        .insert(format!(":{}", k), to_attribute_value(v).ok().unwrap());
                }
                expression_attribute_name.insert(format!("#{}", k), k.to_string());
                (
                    update,
                    remove,
                    expression_attribute_name,
                    expression_attribute_value,
                )
            },
        );

    let update_expression = if !update.is_empty() {
        format!("SET {} ", update.join(", "))
    } else {
        "".into()
    };

    let remove_expression = if !remove.is_empty() {
        format!("REMOVE {} ", remove.join(", "))
    } else {
        "".into()
    };

    let update_expression = format!("{}{}", update_expression, remove_expression);

    if update_expression.is_empty() {
        return Ok(());
    }

    let _ = client
        .update_item()
        .table_name(TABLE_NAME.to_string())
        .key(PK.to_string(), AttributeValue::S(id))
        .update_expression(update_expression)
        .set_expression_attribute_names(Some(expression_attribute_name))
        .set_expression_attribute_values(Some(expression_attribute_value))
        .send()
        .await
        .map_err(|e| {
            tracing::error!("error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(())
}
