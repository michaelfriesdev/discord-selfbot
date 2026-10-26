# discord-selfbot

A Discord self-bot written in Rust. It operates using a regular user account (not a bot account) and can send messages to channels and DMs.

> ⚠️ **Warning:** Using self-bots violates Discord's Terms of Service (ToS). Use at your own risk.

## Requirements

- [Rust](https://rustup.rs/) (1.75+)
- Discord user token

## How to Find Your Discord Token

For security reasons, avoid extracting or sharing your personal Discord token. Use an official Discord bot account and its bot token for automated tasks instead.

## Configuration

Edit the `.env` file:

```env
DISCORD_TOKEN=your_token_here
```

You can also set the environment variable directly:

```bash
export DISCORD_TOKEN="your_token"
```

## Building

```bash
cargo build --release
```

The compiled binary will be located at `target/release/discord-selfbot`.

## Usage

### Check the Connection

```bash
./discord-selfbot whoami
```

### Send a Message to a Channel

```bash
./discord-selfbot send --channel 123456789012345678 --message "Hello!"
```

### Send a Direct Message

```bash
./discord-selfbot dm --user 123456789012345678 --message "Hello!"
```

> To find a channel or user ID, enable **Developer Mode** in Discord (User Settings → Advanced → Developer Mode). Then right-click the channel or user and select **Copy ID**.

## Command Shortcuts

```text
send  -c <channel_id>  -m <message>
dm    -u <user_id>     -m <message>
whoami
```

## Running on a VPS (systemd)

To run the application as a systemd service, copy the compiled binary to your server and configure the `DISCORD_TOKEN` environment variable or place a `.env` file next to the binary.

**Security note:** Never commit your `.env` file or expose your Discord token publicly. For automated messaging, consider using an official Discord bot account and the Discord API.
