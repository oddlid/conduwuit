use super::State;
use crate::{ConduitResult, Database, Error, Result, Ruma};
use ruma::{
    api::client::r0::{
        directory::{
            self, get_public_rooms, get_public_rooms_filtered, get_room_visibility,
            set_room_visibility,
        },
        room,
    },
    events::{
        room::{avatar, canonical_alias, guest_access, history_visibility, name, topic},
        EventType,
    },
    Raw,
};

#[cfg(feature = "conduit_bin")]
use rocket::{get, post, put};

#[cfg_attr(
    feature = "conduit_bin",
    get("/_matrix/client/r0/publicRooms", data = "<body>")
)]
pub async fn get_public_rooms_route(
    db: State<'_, Database>,
    body: Ruma<get_public_rooms::Request>,
) -> ConduitResult<get_public_rooms::Response> {
    let Ruma {
        body:
            get_public_rooms::Request {
                limit,
                server,
                since,
            },
        sender_id,
        device_id,
        json_body,
    } = body;

    let get_public_rooms_filtered::Response {
        chunk,
        prev_batch,
        next_batch,
        total_room_count_estimate,
    } = get_public_rooms_filtered_route(
        db,
        Ruma {
            body: get_public_rooms_filtered::Request {
                filter: None,
                limit,
                room_network: get_public_rooms_filtered::RoomNetwork::Matrix,
                server,
                since,
            },
            sender_id,
            device_id,
            json_body,
        },
    )
    .await?
    .0;

    Ok(get_public_rooms::Response {
        chunk,
        prev_batch,
        next_batch,
        total_room_count_estimate,
    }
    .into())
}

#[cfg_attr(
    feature = "conduit_bin",
    post("/_matrix/client/r0/publicRooms", data = "<_body>")
)]
pub async fn get_public_rooms_filtered_route(
    db: State<'_, Database>,
    _body: Ruma<get_public_rooms_filtered::Request>,
) -> ConduitResult<get_public_rooms_filtered::Response> {
    let mut chunk =
        db.rooms
            .public_rooms()
            .map(|room_id| {
                let room_id = room_id?;

                // TODO: Do not load full state?
                let state = db.rooms.room_state_full(&room_id)?;

                let chunk = directory::PublicRoomsChunk {
                    aliases: Vec::new(),
                    canonical_alias: state
                        .get(&(EventType::RoomCanonicalAlias, "".to_owned()))
                        .map_or(Ok::<_, Error>(None), |s| {
                            Ok(serde_json::from_value::<
                                Raw<canonical_alias::CanonicalAliasEventContent>,
                            >(s.content.clone())
                            .expect("from_value::<Raw<..>> can never fail")
                            .deserialize()
                            .map_err(|_| {
                                Error::bad_database("Invalid canonical alias event in database.")
                            })?
                            .alias)
                        })?,
                    name: state.get(&(EventType::RoomName, "".to_owned())).map_or(
                        Ok::<_, Error>(None),
                        |s| {
                            Ok(serde_json::from_value::<Raw<name::NameEventContent>>(
                                s.content.clone(),
                            )
                            .expect("from_value::<Raw<..>> can never fail")
                            .deserialize()
                            .map_err(|_| {
                                Error::bad_database("Invalid room name event in database.")
                            })?
                            .name()
                            .map(|n| n.to_owned()))
                        },
                    )?,
                    num_joined_members: (db.rooms.room_members(&room_id).count() as u32).into(),
                    room_id,
                    topic: state.get(&(EventType::RoomTopic, "".to_owned())).map_or(
                        Ok::<_, Error>(None),
                        |s| {
                            Ok(Some(
                                serde_json::from_value::<Raw<topic::TopicEventContent>>(
                                    s.content.clone(),
                                )
                                .expect("from_value::<Raw<..>> can never fail")
                                .deserialize()
                                .map_err(|_| {
                                    Error::bad_database("Invalid room topic event in database.")
                                })?
                                .topic,
                            ))
                        },
                    )?,
                    world_readable: state
                        .get(&(EventType::RoomHistoryVisibility, "".to_owned()))
                        .map_or(Ok::<_, Error>(false), |s| {
                            Ok(serde_json::from_value::<
                                Raw<history_visibility::HistoryVisibilityEventContent>,
                            >(s.content.clone())
                            .expect("from_value::<Raw<..>> can never fail")
                            .deserialize()
                            .map_err(|_| {
                                Error::bad_database(
                                    "Invalid room history visibility event in database.",
                                )
                            })?
                            .history_visibility
                                == history_visibility::HistoryVisibility::WorldReadable)
                        })?,
                    guest_can_join: state
                        .get(&(EventType::RoomGuestAccess, "".to_owned()))
                        .map_or(Ok::<_, Error>(false), |s| {
                            Ok(
                            serde_json::from_value::<Raw<guest_access::GuestAccessEventContent>>(
                                s.content.clone(),
                            )
                            .expect("from_value::<Raw<..>> can never fail")
                            .deserialize()
                            .map_err(|_| {
                                Error::bad_database("Invalid room guest access event in database.")
                            })?
                            .guest_access
                                == guest_access::GuestAccess::CanJoin,
                        )
                        })?,
                    avatar_url: state
                        .get(&(EventType::RoomAvatar, "".to_owned()))
                        .map(|s| {
                            Ok::<_, Error>(
                                serde_json::from_value::<Raw<avatar::AvatarEventContent>>(
                                    s.content.clone(),
                                )
                                .expect("from_value::<Raw<..>> can never fail")
                                .deserialize()
                                .map_err(|_| {
                                    Error::bad_database("Invalid room avatar event in database.")
                                })?
                                .url,
                            )
                        })
                        .transpose()?,
                };
                Ok(chunk)
            })
            .filter_map(|r: Result<_>| r.ok()) // Filter out buggy rooms
            // We need to collect all, so we can sort by member count
            .collect::<Vec<_>>();

    chunk.sort_by(|l, r| r.num_joined_members.cmp(&l.num_joined_members));

    /*
    chunk.extend_from_slice(
        &server_server::send_request(
            &db,
            "privacytools.io".to_owned(),
            ruma::api::federation::v1::get_public_rooms::Request {
                limit: Some(20_u32.into()),
                since: None,
                room_network: ruma::api::federation::v1::get_public_rooms::RoomNetwork::Matrix,
            },
        )
        .await
        ?
        .chunk
        .into_iter()
        .map(|c| serde_json::from_str(&serde_json::to_string(&c)?)?)
        .collect::<Vec<_>>(),
    );
    */

    let total_room_count_estimate = (chunk.len() as u32).into();

    Ok(get_public_rooms_filtered::Response {
        chunk,
        prev_batch: None,
        next_batch: None,
        total_room_count_estimate: Some(total_room_count_estimate),
    }
    .into())
}

#[cfg_attr(
    feature = "conduit_bin",
    put("/_matrix/client/r0/directory/list/room/<_>", data = "<body>")
)]
pub async fn set_room_visibility_route(
    db: State<'_, Database>,
    body: Ruma<set_room_visibility::Request>,
) -> ConduitResult<set_room_visibility::Response> {
    match body.visibility {
        room::Visibility::Public => db.rooms.set_public(&body.room_id, true)?,
        room::Visibility::Private => db.rooms.set_public(&body.room_id, false)?,
    }

    Ok(set_room_visibility::Response.into())
}

#[cfg_attr(
    feature = "conduit_bin",
    get("/_matrix/client/r0/directory/list/room/<_>", data = "<body>")
)]
pub async fn get_room_visibility_route(
    db: State<'_, Database>,
    body: Ruma<get_room_visibility::Request>,
) -> ConduitResult<get_room_visibility::Response> {
    Ok(get_room_visibility::Response {
        visibility: if db.rooms.is_public_room(&body.room_id)? {
            room::Visibility::Public
        } else {
            room::Visibility::Private
        },
    }
    .into())
}
