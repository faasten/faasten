//! "init" code for any function invocation. It is responsible for resolving the passed-in
//! gate path in a secure way. That is, it makes sure to set the privilege and the clearance
//! for a logged in user or a public user. Moreover, it tracks the label for the file system
//! traversal and set LabeledInvoke.label to the new label after the traversal.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

use labeled::buckle::{Buckle, Component};
use labeled::{buckle, HasPrivilege};
use log::{debug, error};
use rouille::{input::post::BufferedFile, Request, Response};
use snapfaas::blobstore;
use snapfaas::fs::BackingStore;
use snapfaas::{
    blobstore::Blobstore,
    fs::{self, FS},
    sched::{self, message::LabeledInvoke},
};

pub fn init<S: BackingStore>(
    login: Option<Component>,
    gate_path: String,
    request: &Request,
    sched_conn: &mut TcpStream,
    fs: &FS<S>,
    blobstore: Arc<Mutex<Blobstore>>,
) -> Result<Response, Response> {
    let (payload, blob, label, headers) = prepare_payload(request, blobstore)?;
    let privilege = login.unwrap_or(Component::dc_true());

    {
        fs::utils::clear_label();
        fs::utils::set_my_privilge(privilege);
        if let Some(label) = label {
            fs::utils::taint_with_label(label);
        }
    }

    let req = prepare_labeled_invoke(gate_path, blob, payload, headers, fs)?;
    wait_for_completion(req, sched_conn)
}

fn prepare_payload(
    request: &Request,
    blobstore: Arc<Mutex<Blobstore>>,
) -> Result<
    (
        Vec<u8>,
        HashMap<String, blobstore::Blob>,
        Option<buckle::Buckle>,
        HashMap<String, String>,
    ),
    Response,
> {
    // Parse input into a 3-tuple. support two content types:
    // * multipart/form-data
    // * application/json (passthrough)
    use core::str::FromStr;
    let (files, payload, label, headers) = {
        let content_type = mime::Mime::from_str(
            request.header("content-type").ok_or(
                Response::json(&serde_json::json!({"error": "Missing header content-type"}))
                    .with_status_code(400),
            )?,
        )
        .map_err(|_| {
            Response::json(&serde_json::json!({"error": "Unknown MIME"})).with_status_code(415)
        })?;
        let headers: HashMap<String, String> = request
            .headers()
            .filter_map(|(k, v)| {
                if k.eq_ignore_ascii_case("authorization") {
                    None
                } else {
                    Some((k.to_ascii_lowercase(), v.to_string()))
                }
            })
            .collect();
        // get rid of the `boundary` param in multipart/form-data
        let essence_type = mime::Mime::from_str(content_type.essence_str()).unwrap();
        if essence_type == mime::MULTIPART_FORM_DATA {
            let parsed_form = rouille::post_input!(&request, {
                blob: Vec<BufferedFile>, payload: String, label: Option<String>
            })
            .map_err(|e| {
                Response::json(&serde_json::json!({"error": format!("{:?}", e)}))
                    .with_status_code(400)
            })?;
            let label = if let Some(label) = &parsed_form.label {
                Some(buckle::Buckle::parse(label).map_err(|e| {
                    Response::json(&serde_json::json!({"error": e.to_string()}))
                        .with_status_code(400)
                })?)
            } else {
                None
            };
            (parsed_form.blob, parsed_form.payload, label, headers)
        } else if essence_type == mime::APPLICATION_JSON {
            let mut payload = String::new();
            let label = request
                .header("x-faasten-label")
                .and_then(|b| Buckle::parse(b).ok());
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
            (vec![], payload, label, headers)
        } else {
            return Err(Response::json(&serde_json::json!({
                "error": format!("Unsupported content-type: {:?}", content_type)
            }))
            .with_status_code(415));
        }
    };

    // prepare payload
    let val: serde_json::Value = serde_json::from_str(&payload).map_err(|e| {
        Response::json(&serde_json::json!({"error": e.to_string()})).with_status_code(400)
    })?;
    let payload = val;
    let mut blobs = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        let mut newblob = blobstore.lock().unwrap().create().map_err(|e| {
            Response::json(&serde_json::json!({"error": e.to_string()})).with_status_code(500)
        })?;
        newblob.write_all(f.data.as_ref()).map_err(|e| {
            Response::json(&serde_json::json!({"error": e.to_string()})).with_status_code(500)
        })?;
        let blob = blobstore.lock().unwrap().save(newblob).map_err(|e| {
            Response::json(&serde_json::json!({"error": e.to_string()})).with_status_code(500)
        })?;
        blobs.insert(f.filename.clone().unwrap_or(format!("blob{}", i)), blob);
    }
    Ok((payload.to_string().into(), blobs, label, headers))
}

fn prepare_labeled_invoke<S: BackingStore>(
    gate_path: String,
    mut blobs: HashMap<String, blobstore::Blob>,
    payload: Vec<u8>,
    headers: HashMap<String, String>,
    fs: &FS<S>,
) -> Result<sched::message::LabeledInvoke, Response> {
    let path = fs::path::Path::parse(&gate_path).map_err(|_| {
        Response::json(&serde_json::json!({"error": "Invalid path."})).with_status_code(400)
    })?;
    let (f, gate_privilege) =
        fs::utils::resolve_gate_with_clearance_check(fs, path).map_err(|e| {
            Response::json(&serde_json::json!({ "error": format!("{:?}", e) }))
                .with_status_code(400)
        })?;
    let gate_privilege = Some(gate_privilege.into());
    let label = fs::utils::get_current_label();
    let label = label.into();
    let blobs = blobs.drain().map(|(k, v)| (k, v.name)).collect();
    Ok(sched::message::LabeledInvoke {
        function: Some(f.into()),
        label: Some(label),
        gate_privilege,
        payload,
        headers,
        blobs,
        sync: true,
    })
}

fn wait_for_completion(
    invoke: LabeledInvoke,
    sched_conn: &mut TcpStream,
) -> Result<Response, Response> {
    debug!("submitting: {:?}", invoke);
    // submit the labeled_invoke to the scheduler
    sched::rpc::labeled_invoke(sched_conn, invoke).map_err(|e| {
        error!("{:?}", e);
        Response::json(&serde_json::json!({
            "error": "failed to submit invocation to the scheduler",
        }))
        .with_status_code(500)
    })?;

    debug!("waiting...");
    // wait for the return
    let bs = sched::message::read_u8(sched_conn).map_err(|_| {
        Response::json(&serde_json::json!({
            "error": "failed to read the task return",
        }))
        .with_status_code(500)
    })?;

    use prost::Message;
    use snapfaas::sched::message::TaskReturn;
    match TaskReturn::decode(bs.as_slice()) {
        Ok(tr) => {
            if !Into::<Buckle>::into(tr.label.clone().unwrap()).can_flow_to_with_privilege(
                &fs::utils::get_current_label(),
                &fs::utils::get_privilege(),
            ) {
                Err(Response::json(&serde_json::json!({
                    "error": "unauthorized to read response",
                    "label": format!("{:?}", Into::<Buckle>::into(tr.label.unwrap())),
                    "current_label": format!("{:?}", fs::utils::get_current_label()),
                    "privilege": format!("{:?}", fs::utils::get_privilege())
                }))
                .with_status_code(401))
            } else {
                let resp: Response = tr.into();
                if resp.is_success() {
                    Ok(resp)
                } else {
                    Err(resp)
                }
            }
        }
        Err(_) => Err(Response::json(&serde_json::json!({
            "error": "failed to decode return from Faasten core"
        }))
        .with_status_code(500)),
    }
}
