use futures::StreamExt;
use lazy_static::lazy_static;
use regex::Regex;
use std::env::var;
use std::error::Error;
use std::sync::Arc;
use tokio::spawn;
use twilight_gateway::Cluster;
use twilight_gateway::Intents;
use twilight_http::Client as HttpClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
	let _ = dotenvy::dotenv();

	let token = var("TOKEN")?;
	let intents
		= Intents::GUILD_MESSAGES
		| Intents::GUILD_MESSAGE_REACTIONS
		| Intents::MESSAGE_CONTENT;

	let http_client = Arc::new(HttpClient::new(token.clone()));
	let (cluster, mut events) = Cluster::builder(token, intents)
		.build()
		.await?;
	let cluster = Arc::new(cluster);
	cluster.up().await;
	let cluster_down = cluster.clone();

	println!("connected!");

	while let Some((_shard_id, event)) = events.next().await {
		use twilight_model::gateway::event::Event::*;

		let http_client = Arc::clone(&http_client);
		spawn(async move {
			match event {
				MessageCreate(msg) => {
					lazy_static! {
						static ref TWITTER_LINK_REGEX: Regex = {
							Regex::new(
								// capturing group `goodstuff` captures everything but the domain, protocol,
								// and tracking queries, eg.
								// breezypone/status/1562526463777075200
								// mirta_sh/status/1556685663709323266
								r"https?://(?:www\.)?twitter\.com/(?P<goodstuff>[a-zA-Z0-9_]{4,15}/status/\d{0,20})/?"
							).unwrap()
						};
					}

					let captures = TWITTER_LINK_REGEX.captures_iter(&msg.content).collect::<Vec<_>>();
					if captures.is_empty() { return }
					if captures.len() > 3 {
						let _ = http_client.create_message(msg.channel_id)
							.content("Too many twitter links detected. Maximum allowed is 3").unwrap()
							.reply(msg.id)
							.exec().await;
						return
					}

					let mut capture_iter = captures.into_iter();
					match capture_iter.next() {
						Some(capture) => {
							send_message(&msg, &http_client, capture, true).await;
						}
						None => { return }
					}

					for capture in capture_iter {
						send_message(&msg, &http_client, capture, false).await;
					}
				}

				MessageUpdate(msg) => {}
				MessageDelete(msg) => {}
				ReactionAdd(reaction) => {}
				_ => {}
			}
		});
	}

	use tokio::signal::unix::{ signal, SignalKind };
	let mut sigint = signal(SignalKind::interrupt()).unwrap();
	let mut sigterm = signal(SignalKind::terminate()).unwrap();

	tokio::select! {
		// without biased, tokio::select! will choose random branches to poll,
		// which incurs a small cpu cost for the random number generator
		// biased polling is fine here
		biased;

		_ = sigint.recv() => {
			cluster_down.down();
		}
		_ = sigterm.recv() => {
			cluster_down.down();
		}
	}

	println!("shutting down");

	Ok(())
}

async fn send_message<'h>(
	msg: &twilight_model::channel::message::Message,
	http_client: &HttpClient,
	capture: regex::Captures<'h>,
	should_reply: bool
) {
	let reply = format!("https://vxtwitter.com/{}", &capture["goodstuff"]);

	let mut message = http_client.create_message(msg.channel_id)
		.content(&reply).unwrap();
	if should_reply { message = message.reply(msg.id) }

	let _ = message.exec().await;
}
