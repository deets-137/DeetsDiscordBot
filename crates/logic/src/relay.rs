//! The Activity's relay asks the bot one thing at build step 3: is this person in this
//! Activity instance, and in which voice channel? (docs/features/ACTIVITY.md §6.)
//!
//! The relay knows who the person is (Discord's sign-in). It cannot know the call without the
//! bot token, and the bot token never leaves this PC. So the question comes down the host's
//! socket, the host passes it here (`ToLogic::RelayCheck`), and the logic asks Discord.
//!
//! Nothing here acts on DeetsMusic. That is `deetsmusic_remote`, build step 4.

use std::time::Duration;

use lildeets_ipc::ToHost;
use tracing::{info, warn};

use crate::Ctx;

/// Discord adds a person to an instance's list a moment after their page opens.
const NOT_YET_WAIT: Duration = Duration::from_millis(1500);

// The lines a refused person reads in the Activity.
const STARTING: &str = "Lil Deets is starting. Close the Activity and open it again.";
const NO_ANSWER: &str = "Discord did not answer. Close the Activity and open it again.";
const NO_INSTANCE: &str = "Discord does not know this Activity. Close it and open it again.";
const NOT_IN_IT: &str = "Discord says you are not in this Activity. Close it and open it again.";
const NOT_A_VOICE_CHANNEL: &str = "The Activity works in a server's voice channel.";

/// Answers one check. Runs as its own task: a slow Discord must not hold the logic's loop.
pub async fn check(ctx: &Ctx, rid: String, user: u64, instance: String) {
    let answer = decide(ctx, user, &instance).await;
    match &answer {
        Ok(channel) => info!(user, channel, "relay: a person is in the Activity"),
        Err(why) => info!(user, why, "relay: a person was not confirmed"),
    }
    let (channel, reason) = match answer {
        Ok(channel) => (Some(channel), None),
        Err(why) => (None, Some(why.to_owned())),
    };
    ctx.host.tell(ToHost::RelayChecked { rid, channel, reason }).await;
}

async fn decide(ctx: &Ctx, user: u64, instance: &str) -> Result<u64, &'static str> {
    let Some(app) = ctx.app_id.get().copied() else { return Err(STARTING) };
    for attempt in 0..2 {
        let found = match ctx.rest.activity_instance(app, instance).await {
            Ok(found) => found,
            Err(e) => {
                warn!(error = %e, "relay: GET activity-instances failed");
                return Err(NO_ANSWER);
            }
        };
        match found {
            Some(i) if i.users.contains(&user) => return i.channel.ok_or(NOT_A_VOICE_CHANNEL),
            // Not listed yet (or no instance yet): look once more.
            _ if attempt == 0 => tokio::time::sleep(NOT_YET_WAIT).await,
            Some(_) => return Err(NOT_IN_IT),
            None => return Err(NO_INSTANCE),
        }
    }
    Err(NOT_IN_IT)
}
