#![deny(warnings)]
#[macro_use] extern crate log;
extern crate pretty_env_logger;
extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate warp;

use std::env;
use std::sync::{Arc, Mutex};
use warp::{http::StatusCode, Filter};


/// So we don't have to tackle how different database work, we'll just use
/// a simple in-memory DB, a vector synchronized by a mutex.
type Db = Arc<Mutex<Vec<Todo>>>;

#[derive(Debug, Deserialize, Serialize)]
struct Todo {
    id: u64,
    text: String,
    completed: bool,
}

/// Provides a RESTful web server managing some Todos.
///
/// API will be:
///
/// - `GET /todos`: return a JSON list of Todos.
/// - `POST /todos`: create a new Todo.
/// - `PUT /todos/:id`: update a specific Todo.
/// - `DELETE /todos/:id`: delete a specific Todo.
fn main() {
    if env::var_os("RUST_LOG").is_none() {
        // Set `RUST_LOG=todos=debug` to see debug logs,
        // this only shows access logs.
        env::set_var("RUST_LOG", "todos=info");
    }
    pretty_env_logger::init();

    // These are some `Filter`s that several of the endpoints share,
    // so we'll define them here and reuse them below...


    // Turn our "state", our db, into a Filter so we can combine it
    // easily with others...
    let db = Arc::new(Mutex::new(Vec::<Todo>::new()));
    let db = warp::any().map(move || db.clone());

    // Just the path segment "todos"...
    let todos = warp::path("todos");

    // Combined with `index`, this means nothing comes after "todos".
    // So, for example: `GET /todos`, but not `GET /todos/32`.
    let todos_index = todos.and(warp::path::index());

    // Combined with an id path parameter, for refering to a specific Todo.
    // For example, `POST /todos/32`, but not `POST /todos/32/something-more`.
    let todos_id = todos
        .and(warp::path::param::<u64>())
        .and(warp::path::index());

    // Next, we'll define each our 4 endpoints:

    // `GET /todos`
    let list = warp::get(
        todos_index
            .and(db.clone())
            .map(list_todos)
    );

    // `POST /todos`
    let create = warp::post(
        todos_index
            .and(warp::body::json())
            .and(db.clone())
            .and_then(create_todo)
    );

    // `PUT /todos/:id`
    let update = warp::put(
        todos_id
            .and(warp::body::json())
            .and(db.clone())
            .and_then(update_todo)
    );

    // `DELETE /todos/:id`
    let delete = warp::delete(
        todos_id
            .and(db.clone())
            .and_then(delete_todo)
    );


    // Combine our endpoints, since we want requests to match any of them:
    let api = list
        .or(create)
        .or(update)
        .or(delete);

    // View access logs by setting `RUST_LOG=todos`.
    let routes = api.with(warp::log("todos"));

    // Start up the server...
    warp::serve(routes)
        .run(([127, 0, 0, 1], 3030));
}

// These are our API handlers, the ends of each filter chain.
// Notice how thanks to using `Filter::and`, we can define a function
// with the exact arguments we'd expect from each filter in the chain.
// No tuples are needed, it's auto flattened for the functions.

/// GET /todos
fn list_todos(db: Db) -> impl warp::Reply {
    // Just return a JSON array of all Todos.
    warp::reply::json(&*db.lock().unwrap())
}

/// POST /todos with JSON body
fn create_todo(create: Todo, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    debug!("create_todo: {:?}", create);

    let mut vec = db
        .lock()
        .unwrap();

    for todo in vec.iter() {
        if todo.id == create.id {
            debug!("    -> id already exists: {}", create.id);
            // Todo with id already exists, return `400 BadRequest`.
            return Err(warp::reject::bad_request());
        }
    }

    // No existing Todo with id, so insert and return `201 Created`.
    vec.push(create);

    Ok(StatusCode::CREATED)
}

/// PUT /todos/:id with JSON body
fn update_todo(id: u64, update: Todo, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    debug!("update_todo: id={}, todo={:?}", id, update);
    let mut vec = db
        .lock()
        .unwrap();

    // Look for the specified Todo...
    for todo in vec.iter_mut() {
        if todo.id == id {
            *todo = update;
            return Ok(warp::reply());
        }
    }

    debug!("    -> todo id not found!");

    // If the for loop didn't return OK, then the ID doesn't exist...
    Err(warp::reject::not_found())
}

/// DELETE /todos/:id
fn delete_todo(id: u64, db: Db) -> Result<impl warp::Reply, warp::Rejection> {
    debug!("delete_todo: id={}", id);

    let mut vec = db
        .lock()
        .unwrap();

    let len = vec.len();
    vec.retain(|todo| {
        // Retain all Todos that aren't this id...
        // In other words, remove all that *are* this id...
        todo.id != id
    });

    // If the vec is smaller, we found and deleted a Todo!
    let deleted = vec.len() != len;

    if deleted {
        // respond with a `204 No Content`, which means successful,
        // yet no body expected...
        Ok(StatusCode::NO_CONTENT)
    } else {
        debug!("    -> todo id not found!");
        // Reject this request with a `404 Not Found`...
        Err(warp::reject::not_found())
    }
}

