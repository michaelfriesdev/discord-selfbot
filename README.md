# discord-selfbot

Discord self-bot napisany w Rust. Działa z konta użytkownika (nie bota) — wysyła wiadomości do kanałów i DM.

> ⚠️ **Uwaga:** Używanie self-botów narusza regulamin Discord (ToS). Używaj na własne ryzyko.

## Wymagania

- [Rust](https://rustup.rs/) (1.75+)
- Token użytkownika Discord

## Jak znaleźć token Discord

1. Otwórz Discord w przeglądarce (discord.com)
2. Naciśnij `F12` → zakładka **Network**
3. Wyszukaj dowolny request do `discord.com/api`
4. W headerach znajdź `Authorization` — to jest twój token

## Konfiguracja

Edytuj plik `.env`:

```env
DISCORD_TOKEN=twoj_token_tutaj
```

Możesz też ustawić zmienną środowiskową bezpośrednio:

```bash
export DISCORD_TOKEN="twoj_token"
```

## Budowanie

```bash
cargo build --release
```

Binarka znajdzie się w `target/release/discord-selfbot`.

## Użycie

### Sprawdź połączenie

```bash
./discord-selfbot whoami
```

### Wyślij wiadomość do kanału

```bash
./discord-selfbot send --channel 123456789012345678 --message "Hej!"
```

### Wyślij DM do użytkownika

```bash
./discord-selfbot dm --user 123456789012345678 --message "Hej!"
```

> Jak znaleźć ID kanału/użytkownika: w Discord włącz **Tryb dewelopera** (Ustawienia → Zaawansowane → Tryb dewelopera), następnie kliknij prawym przyciskiem na kanał/użytkownika → **Kopiuj ID**.

## Skróty

```
send  -c <channel_id>  -m <message>
dm    -u <user_id>     -m <message>
whoami
```

## Na VPS (systemd)

Jeśli chcesz uruchamiać bota jako usługę, skopiuj binarke na serwer i ustaw `DISCORD_TOKEN` w środowisku lub pliku `.env` obok binarki.
