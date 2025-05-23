use clap::Subcommand;
use conduwuit::{Result, utils::ReadyExt};
use futures::StreamExt;
use ruma::{RoomId, events::room::message::RoomMessageEventContent};

use crate::{admin_command, admin_command_dispatch};

#[admin_command_dispatch]
#[derive(Debug, Subcommand)]
pub(crate) enum RoomInfoCommand {
	/// - List joined members in a room
	ListJoinedMembers {
		room_id: Box<RoomId>,

		/// Lists only our local users in the specified room
		#[arg(long)]
		local_only: bool,
	},

	/// - Displays room topic
	///
	/// Room topics can be huge, so this is in its
	/// own separate command
	ViewRoomTopic {
		room_id: Box<RoomId>,
	},
}

#[admin_command]
async fn list_joined_members(
	&self,
	room_id: Box<RoomId>,
	local_only: bool,
) -> Result<RoomMessageEventContent> {
	let room_name = self
		.services
		.rooms
		.state_accessor
		.get_name(&room_id)
		.await
		.unwrap_or_else(|_| room_id.to_string());

	let member_info: Vec<_> = self
		.services
		.rooms
		.state_cache
		.room_members(&room_id)
		.ready_filter(|user_id| {
			local_only
				.then(|| self.services.globals.user_is_local(user_id))
				.unwrap_or(true)
		})
		.map(ToOwned::to_owned)
		.filter_map(|user_id| async move {
			Some((
				self.services
					.users
					.displayname(&user_id)
					.await
					.unwrap_or_else(|_| user_id.to_string()),
				user_id,
			))
		})
		.collect()
		.await;

	let output_plain = format!(
		"{} Members in Room \"{}\":\n```\n{}\n```",
		member_info.len(),
		room_name,
		member_info
			.into_iter()
			.map(|(displayname, mxid)| format!("{mxid} | {displayname}"))
			.collect::<Vec<_>>()
			.join("\n")
	);

	Ok(RoomMessageEventContent::notice_markdown(output_plain))
}

#[admin_command]
async fn view_room_topic(&self, room_id: Box<RoomId>) -> Result<RoomMessageEventContent> {
	let Ok(room_topic) = self
		.services
		.rooms
		.state_accessor
		.get_room_topic(&room_id)
		.await
	else {
		return Ok(RoomMessageEventContent::text_plain("Room does not have a room topic set."));
	};

	Ok(RoomMessageEventContent::notice_markdown(format!(
		"Room topic:\n```\n{room_topic}\n```"
	)))
}
