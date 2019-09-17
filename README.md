## Setup

Create a Bot on Slack, fill the api keys in bridge_config.toml.

Create a bot on Keybase (same flow as a normal account). Create a paperkey for the bot. Fill in the values in bridge_config.toml.

Set the environment variable of BRIDGE_CONFIG to the path of the config file.
Bonus points if you use KBFS to securely store the config! (It contains secrets, so don't commit it!)
e.g.: `BRIDGE_CONFIG=/keybase/private/marcopolo,cryptic_msngr/bridge_config.toml cargo run`
