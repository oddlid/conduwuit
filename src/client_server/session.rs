use super::State;
use super::{DEVICE_ID_LENGTH, TOKEN_LENGTH};
use crate::{utils, ConduitResult, Database, Error, Ruma};
use ruma::{
    api::client::{
        error::ErrorKind,
        r0::session::{get_login_types, login, logout, logout_all},
    },
    UserId,
};

#[cfg(feature = "conduit_bin")]
use rocket::{get, post};

#[cfg_attr(feature = "conduit_bin", get("/_matrix/client/r0/login"))]
pub fn get_login_route() -> ConduitResult<get_login_types::Response> {
    Ok(get_login_types::Response {
        flows: vec![get_login_types::LoginType::Password],
    }
    .into())
}

#[cfg_attr(
    feature = "conduit_bin",
    post("/_matrix/client/r0/login", data = "<body>")
)]
pub fn login_route(
    db: State<'_, Database>,
    body: Ruma<login::Request>,
) -> ConduitResult<login::Response> {
    // Validate login method
    let user_id =
        // TODO: Other login methods
        if let (login::UserInfo::MatrixId(username), login::LoginInfo::Password { password }) =
            (body.user.clone(), body.login_info.clone())
        {
            let user_id = UserId::parse_with_server_name(username, db.globals.server_name())
                .map_err(|_| Error::BadRequest(
                    ErrorKind::InvalidUsername,
                    "Username is invalid."
                ))?;
            let hash = db.users.password_hash(&user_id)?
                .ok_or(Error::BadRequest(
                    ErrorKind::Forbidden,
                    "Wrong username or password."
                ))?;

            if hash.is_empty() {
                return Err(Error::BadRequest(
                    ErrorKind::UserDeactivated,
                    "The user has been deactivated"
                ));
            }

            let hash_matches =
                argon2::verify_encoded(&hash, password.as_bytes()).unwrap_or(false);

            if !hash_matches {
                return Err(Error::BadRequest(ErrorKind::Forbidden, "Wrong username or password."));
            }

            user_id
        } else {
            return Err(Error::BadRequest(ErrorKind::Forbidden, "Bad login type."));
        };

    // Generate new device id if the user didn't specify one
    let device_id = body
        .body
        .device_id
        .clone()
        .unwrap_or_else(|| utils::random_string(DEVICE_ID_LENGTH).into());

    // Generate a new token for the device
    let token = utils::random_string(TOKEN_LENGTH);

    // Add device
    db.users.create_device(
        &user_id,
        &device_id,
        &token,
        body.initial_device_display_name.clone(),
    )?;

    Ok(login::Response {
        user_id,
        access_token: token,
        home_server: Some(db.globals.server_name().to_owned()),
        device_id,
        well_known: None,
    }
    .into())
}

#[cfg_attr(
    feature = "conduit_bin",
    post("/_matrix/client/r0/logout", data = "<body>")
)]
pub fn logout_route(
    db: State<'_, Database>,
    body: Ruma<logout::Request>,
) -> ConduitResult<logout::Response> {
    let sender_id = body.sender_id.as_ref().expect("user is authenticated");
    let device_id = body.device_id.as_ref().expect("user is authenticated");

    db.users.remove_device(&sender_id, device_id)?;

    Ok(logout::Response.into())
}

#[cfg_attr(
    feature = "conduit_bin",
    post("/_matrix/client/r0/logout/all", data = "<body>")
)]
pub fn logout_all_route(
    db: State<'_, Database>,
    body: Ruma<logout_all::Request>,
) -> ConduitResult<logout_all::Response> {
    let sender_id = body.sender_id.as_ref().expect("user is authenticated");

    for device_id in db.users.all_device_ids(sender_id) {
        if let Ok(device_id) = device_id {
            db.users.remove_device(&sender_id, &device_id)?;
        }
    }

    Ok(logout_all::Response.into())
}
