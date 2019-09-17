mod bridge_info;
mod keybase_profile;
use bridge_info::get_bridge_info;
use futures::channel::mpsc;
use futures::{executor::block_on, prelude::*};
use htmlescape::decode_html;
use keybase_bot_api::{
    chat::{ChannelParams, Notification},
    Bot as KBBot, Chat,
};
use keybase_protocol::chat1::{self, api};
use slack::{
    api::{
        channels,
        chat::{post_message as post_slack_message, PostMessageRequest},
        requests, users_profile,
    },
    Event, Message, RtmClient,
};

use std::thread;

struct MyHandler {
    oauth: String,
    slack_client: requests::Client,
    slack_msg_channel: Option<mpsc::Sender<SlackMessage>>,
}

#[derive(Debug)]
enum SlackMessage {
    Simple {
        username: String,
        channel_name: String,
        msg_text: String,
    },
}

impl SlackMessage {
    fn channel_name(&self) -> &str {
        match self {
            SlackMessage::Simple { channel_name, .. } => channel_name.as_str(),
        }
    }
}

impl MyHandler {
    fn new(oauth: String) -> Self {
        MyHandler {
            oauth,
            slack_client: requests::default_client().unwrap(),
            slack_msg_channel: None,
        }
    }

    fn slack_msg_stream(&mut self) -> mpsc::Receiver<SlackMessage> {
        let (sender, receiver) = mpsc::channel::<SlackMessage>(128);
        self.slack_msg_channel.replace(sender);
        receiver
    }

    fn get_channel_name(&self, channel_id: &str) -> Option<String> {
        match channels::info(
            &self.slack_client,
            &self.oauth,
            &channels::InfoRequest {
                channel: channel_id,
            },
        ) {
            Ok(channels::InfoResponse {
                channel: Some(channel),
                ..
            }) => channel.name,
            Ok(missing_channel) => {
                println!("Missing channel info: {:?}", missing_channel);
                None
            }
            Err(e) => {
                println!("Error getting channel info: {:?}", e);
                None
            }
        }
    }
}

impl slack::EventHandler for MyHandler {
    fn on_event(&mut self, _cli: &RtmClient, event: Event) {
        match event {
            Event::Message(msg) => match *msg {
                Message::Standard(msg) => {
                    // println!("Msg is {:?}", msg);
                    let channel_name = msg.channel.as_ref().and_then(|c| self.get_channel_name(c));
                    if let (Some(user), Some(text)) = (msg.user, msg.text) {
                        match users_profile::get(
                            &self.slack_client,
                            &self.oauth,
                            &users_profile::GetRequest {
                                user: Some(&user),
                                include_labels: None,
                            },
                        ) {
                            Ok(users_profile::GetResponse {
                                profile: Some(profile),
                                ..
                            }) => {
                                let username = match (profile.first_name, profile.last_name) {
                                    (Some(first), Some(last)) => format!("{} {}", first, last),
                                    (Some(first), None) => first,
                                    (None, Some(last)) => last,
                                    (None, None) => user,
                                };
                                let parsed_msg = SlackMessage::Simple {
                                    username,
                                    channel_name: channel_name
                                        .unwrap_or_else(|| String::from("unkown channel")),
                                    msg_text: decode_html(&text).unwrap_or(text),
                                };

                                if let Some(sender) = self.slack_msg_channel.as_mut() {
                                    if let Err(e) = sender.start_send(parsed_msg) {
                                        println!("Error sending slack msg to mpsc {:?}", e);
                                    }
                                } else {
                                    println!("No listeners for msg: {:?}", parsed_msg)
                                }
                            }
                            Ok(resp) => {
                                println!("Failed to get user profile: {:?}", resp);
                                println!("{} said {}", user, text)
                            }
                            Err(e) => {
                                println!("Failed to get user ident: {:?}", e);
                                println!("{} said {}", user, text)
                            }
                        }
                    }
                }
                _ => {
                    println!("message event ({:?})", msg);
                }
            },
            _ => {
                println!("on_event(event: {:?})", event);
            }
        }
    }

    fn on_close(&mut self, _cli: &RtmClient) {
        println!("on_close");
    }

    fn on_connect(&mut self, cli: &RtmClient) {
        println!("on_connect");
        // find the general channel id from the `StartResponse`
        let _channel_id = cli
            .start_response()
            .channels
            .as_ref()
            .and_then(|channels| {
                channels.iter().find(|chan| match chan.name {
                    None => false,
                    Some(ref name) => name == "bot-test2",
                })
            })
            .and_then(|chan| chan.id.as_ref())
            .expect("bottest channel not found");
        // let _ = cli.sender().send_message(&channel_id, "Hello world!");
        // Send a message over the real time api websocket
    }
}

fn main() {
    let mut keybase_profile_pics = keybase_profile::KeybaseProfilePictureCache::default();
    // Slack setup
    let bridge_info = get_bridge_info();
    let mut handler = MyHandler::new(bridge_info.slack.oauth_access_token.clone());
    let slack_stream = handler.slack_msg_stream();
    let api_key: String = bridge_info.slackbot.oauth_access_token.clone();
    let join_handle = thread::spawn(move || {
        let r = RtmClient::login_and_run(&api_key, &mut handler);
        match r {
            Ok(_) => {}
            Err(err) => panic!("Error: {}", err),
        }
    });

    // Keybase setup
    let mut kb_bot = KBBot::new(
        &bridge_info.keybase.bot_name,
        &bridge_info.keybase.paper_key,
    )
    .expect("Couldn't login to keybase");
    let kb_msgs = kb_bot
        .listen()
        .expect("failed to start listening to keybase");
    let kb_team = bridge_info.keybase.team.clone();

    // Bridge msgs from Slack -> Keybase
    let slack_bridge_future = slack_stream.for_each(|msg| {
        // println!("Got msg: {:?}", msg);
        let channel = ChannelParams {
            name: kb_team.clone(),
            members_type: Some(String::from("team")),
            topic_name: Some(msg.channel_name().to_string()),
        };
        match msg {
            SlackMessage::Simple {
                msg_text, username, ..
            } => {
                if let Err(e) = kb_bot.send_msg(&channel, &format!("{}: {}", username, msg_text)) {
                    println!("Error sending msg {:?}", e);
                }
            }
        }
        future::ready(())
    });

    // Bridge msgs from Keybase -> Slack
    let slack_msg_sender = requests::default_client().unwrap();
    let kb_bridge_future = kb_msgs.for_each(|notif| {
        // println!("notif is {:?}", notif);
        match notif {
            Notification::Chat(api::MsgNotification {
                msg:
                    Some(api::MsgSummary {
                        content:
                            Some(api::MsgContent {
                                text:
                                    // Why is this private??
                                    // Some(api::MessageText{
                                    Some(chat1::MessageText {
                                        body: Some(msg_text),
                                        ..
                                    }),
                                ..
                            }),
                        channel:
                            Some(api::ChatChannel {
                                name: Some(team_name),
                                topicName: Some(channel_name),
                                ..
                            }),
                        sender:
                            Some(api::MsgSender {
                                username: Some(username),
                                ..
                            }),
                        ..
                    }),
                ..
            }) => {
                // println!("Got KB Msg: {} in {}#{}: {}", username, team_name, channel_name, msg_text);
                if team_name == kb_team && username != bridge_info.keybase.bot_name {
                    let profile_pic: Option<&str> = keybase_profile_pics.get_keybase_profile_picture(&username).ok().map(|s| s.as_str());
                    if let Err(e) = post_slack_message(&slack_msg_sender, &bridge_info.slackbot.oauth_access_token, &PostMessageRequest {
                        channel: &channel_name,
                        username: Some(&username),
                        text: &msg_text,
                        icon_url: profile_pic,
                        ..Default::default()
                    }) {
                        println!("Error in posting msg to slack {:?}", e);
                    }
                } else {
                    println!("Got a msg from a team name that I haven't been configured for: {}", team_name);
                }
            }
            _ => println!("Unhandled notification: {:?}", notif),
        };
        future::ready(())
    });

    block_on(future::join(slack_bridge_future, kb_bridge_future));
    join_handle.join().unwrap();
}
