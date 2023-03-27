//! "init" code for any function invocation. It is responsible for resolving the passed-in
//! gate path in a secure way. That is, it makes sure to set the privilege and the clearance
//! for a logged in user or a public user. Moreover, it tracks the label for the file system
//! traversal and set LabeledInvoke.label to the new label after the traversal.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

use labeled::buckle;
use rouille::{input::post::BufferedFile, Request, Response};
use snapfaas::syscall_server::{buckle_to_pblabel, component_to_pbcomponent};
use snapfaas::{
    blobstore::Blobstore,
    fs::{self, FS},
    sched::{self, message::LabeledInvoke},
};

pub fn init(
    login: Option<String>,
    gate_path: String,
    request: &Request,
    sched_conn: &mut TcpStream,
    fs: &FS<&lmdb::Environment>,
    blobstore: Arc<Mutex<Blobstore>>,
) -> Result<Response, Response> {
    let (payload, label) = prepare_payload(request, blobstore)?;
    let (privilege, clearance) = match login {
        Some(login) => {
            let user_principal = vec![login.clone()];
            let user_root_privilege = [buckle::Clause::new_from_vec(vec![user_principal])].into();
            (
                user_root_privilege,
                buckle::Buckle::parse(&(login + ",T")).unwrap(),
            )
        }
        None => (buckle::Component::dc_true(), buckle::Buckle::public()),
    };

    {
        fs::utils::clear_label();
        fs::utils::taint_with_label(label);
        fs::utils::set_my_privilge(privilege);
        fs::utils::set_clearance(clearance);
    }

    let req = prepare_labeled_invoke(gate_path, payload, fs)?;
    wait_for_completion(req, sched_conn)
}

fn prepare_payload(
    request: &Request,
    blobstore: Arc<Mutex<Blobstore>>,
) -> Result<(String, buckle::Buckle), Response> {
    // Parse input into a 3-tuple. support two content types:
    // * multipart/form-data
    // * application/json (passthrough)
    use core::str::FromStr;
    let (maybe_file, payload, label) = {
        let content_type = mime::Mime::from_str(
            request.header("content-type").ok_or(
                Response::json(&serde_json::json!({"error": "Missing header content-type"}))
                    .with_status_code(400),
            )?,
        )
        .map_err(|_| {
            Response::json(&serde_json::json!({"error": "Unknown MIME"})).with_status_code(415)
        })?;
        // get rid of the `boundary` param in multipart/form-data
        let essence_type = mime::Mime::from_str(content_type.essence_str()).unwrap();
        if essence_type == mime::MULTIPART_FORM_DATA {
            let parsed_form = rouille::post_input!(&request, {
                file: Option<BufferedFile>, payload: String, label: String
            })
            .map_err(|e| {
                Response::json(&serde_json::json!({"error": e.to_string()})).with_status_code(400)
            })?;
            let label = buckle::Buckle::parse(&parsed_form.label).map_err(|e| {
                Response::json(&serde_json::json!({"error": e.to_string()})).with_status_code(400)
            })?;
            (parsed_form.file, parsed_form.payload, label)
        } else if essence_type == mime::APPLICATION_JSON {
            let mut payload = String::new();
            let _ = request
                .data()
                .unwrap()
                .read_to_string(&mut payload)
                .map_err(|e| {
                    Response::json(&serde_json::json!({
                        "error": e.to_string()
                    }))
                    .with_status_code(400)
                })?;
            (None, payload, buckle::Buckle::public())
        } else {
            return Err(Response::json(&serde_json::json!({
                "error":
                    format!(
                        "Unsupported content-type: {:?}",
                        content_type
                    )
            }))
            .with_status_code(415));
        }
    };

    // prepare payload
    let val: serde_json::Value = serde_json::from_str(&payload).map_err(|e| {
        Response::json(&serde_json::json!({"error": e.to_string()})).with_status_code(400)
    })?;
    let mut payload = serde_json::json!({ "input": val });
    if let Some(f) = maybe_file {
        // store file into the blobstore
        let mut newblob = blobstore.lock().unwrap().create().map_err(|e| {
            Response::json(&serde_json::json!({"error": e.to_string()})).with_status_code(500)
        })?;
        newblob.write_all(f.data.as_ref()).map_err(|e| {
            Response::json(&serde_json::json!({"error": e.to_string()})).with_status_code(500)
        })?;
        let name = blobstore
            .lock()
            .unwrap()
            .save(newblob)
            .map_err(|e| {
                Response::json(&serde_json::json!({"error": e.to_string()})).with_status_code(500)
            })?
            .name;
        payload
            .as_object_mut()
            .unwrap()
            .insert("input-blob".to_string(), serde_json::Value::String(name));
    };
    Ok((payload.to_string(), label))
}

fn prepare_labeled_invoke(
    gate_path: String,
    payload: String,
    fs: &FS<&lmdb::Environment>,
) -> Result<sched::message::LabeledInvoke, Response> {
    let path = fs::path::Path::parse(&gate_path).map_err(|_| {
        Response::json(&serde_json::json!({"error": "Invalid path."})).with_status_code(400)
    })?;
    let (f, gate_privilege) = fs::utils::invoke_clearance_check(fs, path).map_err(|e| {
        Response::json(&serde_json::json!({ "error": format!("{:?}", e) })).with_status_code(400)
    })?;
    let gate_privilege = component_to_pbcomponent(&gate_privilege);
    let label = fs::utils::get_current_label();
    let label = buckle_to_pblabel(&label);
    Ok(sched::message::LabeledInvoke {
        function: Some(f.into()),
        label: Some(label),
        gate_privilege,
        payload: payload.to_string(),
        sync: true,
    })
}

fn wait_for_completion(
    invoke: LabeledInvoke,
    sched_conn: &mut TcpStream,
) -> Result<Response, Response> {
    // submit the labeled_invoke to the scheduler
    sched::rpc::labeled_invoke(sched_conn, invoke).map_err(|_| {
        Response::json(&serde_json::json!({
            "error": "failed to submit invocation to the scheduler",
        }))
        .with_status_code(500)
    })?;

    // wait for the return
    let ret = sched::message::read_u8(sched_conn).map_err(|_| {
        Response::json(&serde_json::json!({
            "error": "failed to read the task return",
        }))
        .with_status_code(500)
    })?;
    Ok(Response::from_data("application/octet-stream", ret))
}
