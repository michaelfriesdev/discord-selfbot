use anyhow::{Context, Result};
use chrono::{Datelike, Local, Timelike};
use clap::{Parser, Subcommand};
use dotenvy::dotenv;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use tokio::time::{sleep, Duration};

const COUNTER_CHANNEL_ID: &str = "1530191197178695700";
const WEATHER_CHANNEL_ID: &str = "1526657004649844796";
const LAST_LETTER_CHANNEL_ID: &str = "1548673440096067704";
const WAIT_MINUTES: u64 = 5;
const WEATHER_INTERVAL_HOURS: u64 = 3;
const PID_FILE: &str = "discord-selfbot.pid";

#[derive(Parser)]
#[command(name = "discord-selfbot")]
#[command(about = "Discord self-bot - wysylanie wiadomosci z konta uzytkownika")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Send {
        #[arg(short, long)]
        channel: String,
        #[arg(short, long)]
        message: String,
    },
    Dm {
        #[arg(short, long)]
        user: String,
        #[arg(short, long)]
        message: String,
    },
    Whoami,
    Counter,
    Weather,
    LastLetter,
    Stop,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoResponse {
    current: OpenMeteoCurrent,
    hourly: OpenMeteoHourly,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoCurrent {
    temperature_2m: f64,
    relative_humidity_2m: u32,
    wind_speed_10m: f64,
    weather_code: u32,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoHourly {
    time: Vec<String>,
    precipitation_probability: Vec<Option<u32>>,
}

#[derive(Debug, Deserialize)]
struct DiscordUser {
    id: String,
    username: String,
    discriminator: String,
    global_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct SendMessagePayload {
    content: String,
}

#[derive(Debug, Deserialize)]
struct DiscordMessage {
    id: String,
    content: String,
    author: MessageAuthor,
}

#[derive(Debug, Deserialize)]
struct MessageAuthor {
    id: String,
}

#[derive(Debug, Deserialize)]
struct DmChannel {
    id: String,
}

#[derive(Debug, Serialize)]
struct CreateDmPayload {
    recipient_id: String,
}

struct DiscordClient {
    http: reqwest::Client,
}

impl DiscordClient {
    fn new(token: String) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&token)
                .context("Nieprawidlowy token - zawiera niedozwolone znaki")?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "User-Agent",
            HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            ),
        );

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("Nie udalo sie stworzyc klienta HTTP")?;

        Ok(Self { http })
    }

    async fn get_me(&self) -> Result<DiscordUser> {
        let resp = self
            .http
            .get("https://discord.com/api/v10/users/@me")
            .send()
            .await
            .context("Blad polaczenia z Discord API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Discord API zwrocilo blad {}: {}", status, body);
        }

        resp.json::<DiscordUser>()
            .await
            .context("Nie udalo sie sparsowac odpowiedzi")
    }

    async fn get_last_message(&self, channel_id: &str) -> Result<DiscordMessage> {
        let url = format!(
            "https://discord.com/api/v10/channels/{}/messages?limit=1",
            channel_id
        );

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("Blad polaczenia z Discord API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Nie udalo sie pobrac wiadomosci ({}): {}", status, body);
        }

        let mut messages: Vec<DiscordMessage> = resp
            .json()
            .await
            .context("Nie udalo sie sparsowac listy wiadomosci")?;

        messages.pop().context("Kanal jest pusty - brak wiadomosci")
    }

    async fn send_message(&self, channel_id: &str, content: &str) -> Result<DiscordMessage> {
        let url = format!(
            "https://discord.com/api/v10/channels/{}/messages",
            channel_id
        );

        let payload = SendMessagePayload {
            content: content.to_string(),
        };

        let resp = self
            .http
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Blad polaczenia z Discord API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Nie udalo sie wyslac wiadomosci ({}): {}", status, body);
        }

        resp.json::<DiscordMessage>()
            .await
            .context("Nie udalo sie sparsowac odpowiedzi")
    }

    async fn open_dm(&self, user_id: &str) -> Result<DmChannel> {
        let payload = CreateDmPayload {
            recipient_id: user_id.to_string(),
        };

        let resp = self
            .http
            .post("https://discord.com/api/v10/users/@me/channels")
            .json(&payload)
            .send()
            .await
            .context("Blad polaczenia z Discord API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Nie udalo sie otworzyc DM ({}): {}", status, body);
        }

        resp.json::<DmChannel>()
            .await
            .context("Nie udalo sie sparsowac odpowiedzi")
    }
}

fn pid_file_path() -> PathBuf {
    PathBuf::from(PID_FILE)
}

fn write_pid() {
    let pid = std::process::id();
    if let Err(e) = fs::write(pid_file_path(), pid.to_string()) {
        eprintln!("Ostrzezenie: nie udalo sie zapisac PID do pliku: {}", e);
    } else {
        println!("PID {} zapisany do {}", pid, PID_FILE);
    }
}

fn remove_pid_file() {
    let _ = fs::remove_file(pid_file_path());
}

fn stop_bot() -> Result<()> {
    let path = pid_file_path();
    let pid_str = fs::read_to_string(&path)
        .with_context(|| format!("Nie znaleziono pliku PID: {}\nBot nie jest uruchomiony?", PID_FILE))?;

    let pid: u32 = pid_str
        .trim()
        .parse()
        .context("Nieprawidlowy PID w pliku")?;

    println!("Zatrzymuje bota (PID: {})...", pid);

    #[cfg(target_os = "windows")]
    {
        let status = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .status()
            .context("Nie udalo sie wykonac taskkill")?;

        if status.success() {
            println!("Bot zatrzymany.");
            let _ = fs::remove_file(&path);
        } else {
            anyhow::bail!("taskkill zakonczyl sie bledem. Sprawdz czy PID {} nadal istnieje.", pid);
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let status = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .context("Nie udalo sie wykonac kill")?;

        if status.success() {
            println!("Bot zatrzymany.");
            let _ = fs::remove_file(&path);
        } else {
            anyhow::bail!("kill zakonczyl sie bledem. Sprawdz czy PID {} nadal istnieje.", pid);
        }
    }

    Ok(())
}

fn weather_code_to_description(code: u32) -> &'static str {
    match code {
        0 => "Bezchmurnie",
        1 => "Przeważnie bezchmurnie",
        2 => "Częściowe zachmurzenie",
        3 => "Zachmurzenie całkowite",
        45 | 48 => "Mgła",
        51 | 53 => "Lekka mżawka",
        55 => "Gęsta mżawka",
        61 => "Słaby deszcz",
        63 => "Umiarkowany deszcz",
        65 => "Silny deszcz",
        71 => "Słabe opady śniegu",
        73 => "Umiarkowane opady śniegu",
        75 => "Silne opady śniegu",
        77 => "Ziarna śniegu",
        80 => "Słabe przelotne opady",
        81 => "Umiarkowane przelotne opady",
        82 => "Gwałtowne przelotne opady",
        85 | 86 => "Przelotne opady śniegu",
        95 => "Burza",
        96 | 99 => "Burza z gradem",
        _ => "Brak danych",
    }
}

fn weekday_pl(weekday: chrono::Weekday) -> &'static str {
    match weekday {
        chrono::Weekday::Mon => "poniedziałek",
        chrono::Weekday::Tue => "wtorek",
        chrono::Weekday::Wed => "środa",
        chrono::Weekday::Thu => "czwartek",
        chrono::Weekday::Fri => "piątek",
        chrono::Weekday::Sat => "sobota",
        chrono::Weekday::Sun => "niedziela",
    }
}

async fn fetch_weather(http: &reqwest::Client) -> Result<OpenMeteoResponse> {
    let url = "https://api.open-meteo.com/v1/forecast\
        ?latitude=54.0902\
        &longitude=19.0958\
        &current=temperature_2m,relative_humidity_2m,wind_speed_10m,weather_code\
        &hourly=precipitation_probability\
        &wind_speed_unit=kmh\
        &forecast_days=1";

    let resp = http
        .get(url)
        .send()
        .await
        .context("Blad polaczenia z Open-Meteo")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Open-Meteo API blad ({}): {}", status, body);
    }

    resp.json::<OpenMeteoResponse>()
        .await
        .context("Nie udalo sie sparsowac odpowiedzi Open-Meteo")
}

fn build_weather_message(data: &OpenMeteoResponse) -> String {
    let now = Local::now();
    let hour = now.hour() as usize;
    let day_name = weekday_pl(now.weekday());
    let time_str = now.format("%H:%M").to_string();

    let precip = data
        .hourly
        .precipitation_probability
        .get(hour)
        .and_then(|v| *v)
        .unwrap_or(0);

    let desc = weather_code_to_description(data.current.weather_code);
    let temp = data.current.temperature_2m.round() as i64;
    let humidity = data.current.relative_humidity_2m;
    let wind = data.current.wind_speed_10m.round() as u64;

    format!(
        "**Pogoda w Nowym Dworze Gdańskim:**\n\
         \n\
         Obecnie **{temp}°C** | Szansa opadów: **{precip}%** | Wilgotność: **{humidity}%** | Wiatr: **{wind} km/h**\n\
         \n\
         {day_name}, {time_str}, {desc}\n\
         \n\
         -# wiadomość została wysłana przez bota",
    )
}

fn seconds_until_next_weather_slot() -> u64 {
    let now = Local::now();
    let hour = now.hour() as u64;
    let minute = now.minute() as u64;
    let second = now.second() as u64;

    let hours_since_last_slot = hour % WEATHER_INTERVAL_HOURS;
    let hours_to_next = WEATHER_INTERVAL_HOURS - hours_since_last_slot;
    let secs_to_next_full_hour = (60 - minute) * 60 - second;

    if hours_since_last_slot == 0 && minute == 0 && second == 0 {
        return WEATHER_INTERVAL_HOURS * 3600;
    }

    secs_to_next_full_hour + (hours_to_next - 1) * 3600
}

fn next_weather_slot_str() -> String {
    let now = Local::now();
    let hour = now.hour() as u64;
    let hours_since_last = hour % WEATHER_INTERVAL_HOURS;
    let next_hour = (hour + WEATHER_INTERVAL_HOURS - hours_since_last) % 24;
    format!("{:02}:00", next_hour)
}

async fn run_weather(client: &DiscordClient) -> Result<()> {
    write_pid();

    tokio::spawn(async {
        tokio::signal::ctrl_c().await.ok();
        remove_pid_file();
        println!("\nZatrzymano. PID usuniety.");
        std::process::exit(0);
    });

    println!(
        "Uruchamiam tryb pogody na kanale {} (co {} godziny, sloty: 0:00/3:00/6:00/9:00...)",
        WEATHER_CHANNEL_ID, WEATHER_INTERVAL_HOURS
    );

    let wait_secs = seconds_until_next_weather_slot();
    println!(
        "Nastepna wyslka za {}h {}min (o {})",
        wait_secs / 3600,
        (wait_secs % 3600) / 60,
        next_weather_slot_str()
    );
    println!("Aby zatrzymac: Ctrl+C lub './discord-selfbot stop'\n");

    sleep(Duration::from_secs(wait_secs)).await;

    loop {
        println!("--- Pobieranie pogody ({})", Local::now().format("%H:%M:%S"));

        match fetch_weather(&client.http).await {
            Err(e) => {
                eprintln!("Blad pobierania pogody: {}", e);
                eprintln!("Ponowie probe za 5 minut...");
                sleep(Duration::from_secs(5 * 60)).await;
                continue;
            }
            Ok(data) => {
                let msg = build_weather_message(&data);
                println!("Wysylam:\n{}\n", msg);

                match client.send_message(WEATHER_CHANNEL_ID, &msg).await {
                    Ok(sent) => println!("Wyslano (ID: {})", sent.id),
                    Err(e) => eprintln!("Blad wysylania: {}", e),
                }
            }
        }

        let wait = seconds_until_next_weather_slot();
        println!(
            "Nastepna wyslka za {}h {}min (o {})\n",
            wait / 3600,
            (wait % 3600) / 60,
            next_weather_slot_str()
        );
        sleep(Duration::from_secs(wait)).await;
    }
}

fn word_for_letter(letter: char) -> Option<&'static str> {
    match letter {
        'a' => Some("Ananas"),
        'ą' => Some("Ąkacja"),
        'b' => Some("Banan"),
        'c' => Some("Cebula"),
        'ć' => Some("Ćma"),
        'd' => Some("Delfin"),
        'e' => Some("Elefant"),
        'ę' => Some("Ęgorz"),
        'f' => Some("Flamingo"),
        'g' => Some("Goryl"),
        'h' => Some("Hipopotam"),
        'i' => Some("Iguana"),
        'j' => Some("Jaguar"),
        'k' => Some("Kaktus"),
        'l' => Some("Lama"),
        'ł' => Some("Łoś"),
        'm' => Some("Małpa"),
        'n' => Some("Nosorożec"),
        'ń' => Some("Ńku"),
        'o' => Some("Orangutan"),
        'ó' => Some("Ósemka"),
        'p' => Some("Papuga"),
        'r' => Some("Ryba"),
        's' => Some("Słoń"),
        'ś' => Some("Świnia"),
        't' => Some("Tygrys"),
        'u' => Some("Ufo"),
        'w' => Some("Wąż"),
        'x' => Some("Xylofonista"),
        'y' => Some("Yak"),
        'z' => Some("Zebra"),
        'ź' => Some("Źrebak"),
        'ż' => Some("Żyrafa"),
        _ => None,
    }
}

fn last_letter(text: &str) -> Option<char> {
    let last_word = text.split_whitespace().last()?;
    last_word
        .chars()
        .rev()
        .find(|c| c.is_alphabetic())
        .map(|c| c.to_lowercase().next().unwrap_or(c))
}

async fn run_last_letter(client: &DiscordClient, my_id: &str) -> Result<()> {
    write_pid();

    tokio::spawn(async {
        tokio::signal::ctrl_c().await.ok();
        remove_pid_file();
        println!("\nZatrzymano. PID usuniety.");
        std::process::exit(0);
    });

    println!(
        "Uruchamiam tryb ostatniej litery na kanale {} (co {} min)",
        LAST_LETTER_CHANNEL_ID, WAIT_MINUTES
    );
    println!("Aby zatrzymac: Ctrl+C lub './discord-selfbot stop'\n");

    loop {
        println!("--- Sprawdzam ostatnia wiadomosc ({})", Local::now().format("%H:%M:%S"));

        match client.get_last_message(LAST_LETTER_CHANNEL_ID).await {
            Err(e) => {
                eprintln!("Blad pobierania wiadomosci: {}", e);
                sleep(Duration::from_secs(WAIT_MINUTES * 60)).await;
                continue;
            }
            Ok(last_msg) => {
                if last_msg.author.id == my_id {
                    println!(
                        "Ostatnia wiadomosc nalezy do nas (\"{}\") - czekam {} min",
                        last_msg.content.trim(),
                        WAIT_MINUTES
                    );
                    sleep(Duration::from_secs(WAIT_MINUTES * 60)).await;
                    continue;
                }

                let content = last_msg.content.trim();
                println!("Ostatnia wiadomosc: \"{}\"", content);

                match last_letter(content) {
                    None => {
                        println!("Nie znaleziono litery w wiadomosci - czekam {} min", WAIT_MINUTES);
                        sleep(Duration::from_secs(WAIT_MINUTES * 60)).await;
                        continue;
                    }
                    Some(letter) => {
                        println!("Ostatnia litera: '{}'", letter);

                        match word_for_letter(letter) {
                            None => {
                                println!(
                                    "Brak slowa dla litery '{}' - czekam {} min",
                                    letter, WAIT_MINUTES
                                );
                                sleep(Duration::from_secs(WAIT_MINUTES * 60)).await;
                                continue;
                            }
                            Some(word) => {
                                println!("Wysylam: \"{}\"", word);

                                match client.send_message(LAST_LETTER_CHANNEL_ID, word).await {
                                    Ok(sent) => println!("Wyslano (ID: {})", sent.id),
                                    Err(e) => eprintln!("Blad wysylania: {}", e),
                                }
                            }
                        }
                    }
                }

                println!("Czekam {} min...\n", WAIT_MINUTES);
                sleep(Duration::from_secs(WAIT_MINUTES * 60)).await;
            }
        }
    }
}

async fn run_counter(client: &DiscordClient, my_id: &str) -> Result<()> {
    write_pid();

    tokio::spawn(async {
        tokio::signal::ctrl_c().await.ok();
        remove_pid_file();
        println!("\nZatrzymano. PID usuniety.");
        std::process::exit(0);
    });

    println!(
        "Uruchamiam tryb licznika na kanale {} (co {} min)",
        COUNTER_CHANNEL_ID, WAIT_MINUTES
    );
    println!("Aby zatrzymac: Ctrl+C lub './discord-selfbot stop'\n");

    loop {
        println!("--- Sprawdzam ostatnia wiadomosc ---");

        match client.get_last_message(COUNTER_CHANNEL_ID).await {
            Err(e) => {
                eprintln!("Blad pobierania wiadomosci: {}", e);
                println!("Czekam {} minut przed ponowna proba...", WAIT_MINUTES);
                sleep(Duration::from_secs(WAIT_MINUTES * 60)).await;
                continue;
            }
            Ok(last_msg) => {
                if last_msg.author.id == my_id {
                    println!(
                        "Ostatnia wiadomosc nalezy do nas (\"{}\") - czekam {} minut",
                        last_msg.content.trim(),
                        WAIT_MINUTES
                    );
                    sleep(Duration::from_secs(WAIT_MINUTES * 60)).await;
                    continue;
                }

                let trimmed = last_msg.content.trim();
                match trimmed.parse::<i64>() {
                    Err(_) => {
                        println!(
                            "Ostatnia wiadomosc \"{}\" nie jest liczba - czekam {} minut",
                            trimmed, WAIT_MINUTES
                        );
                        sleep(Duration::from_secs(WAIT_MINUTES * 60)).await;
                        continue;
                    }
                    Ok(number) => {
                        let next = number + 1;
                        println!(
                            "Ostatnia wiadomosc: {} (od {}), wysylam: {}",
                            number, last_msg.author.id, next
                        );

                        match client.send_message(COUNTER_CHANNEL_ID, &next.to_string()).await {
                            Ok(sent) => println!("Wyslano {} (ID: {})", sent.content, sent.id),
                            Err(e) => eprintln!("Blad wysylania: {}", e),
                        }

                        println!("Czekam {} minut...\n", WAIT_MINUTES);
                        sleep(Duration::from_secs(WAIT_MINUTES * 60)).await;
                    }
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenv();

    let cli = Cli::parse();

    if let Commands::Stop = cli.command {
        return stop_bot();
    }

    let token = env::var("DISCORD_TOKEN")
        .context("Brak DISCORD_TOKEN - ustaw go w pliku .env lub jako zmienna srodowiskowa")?;

    if token == "twoj_token_tutaj" || token.is_empty() {
        anyhow::bail!(
            "Token nie zostal ustawiony!\n\
             Edytuj plik .env i wstaw swoj token Discord."
        );
    }

    let client = DiscordClient::new(token)?;

    match cli.command {
        Commands::Whoami => {
            println!("Laczenie z Discord API...");
            let user = client.get_me().await?;
            let display_name = user
                .global_name
                .unwrap_or_else(|| format!("{}#{}", user.username, user.discriminator));
            println!("Zalogowano jako: {} (ID: {})", display_name, user.id);
        }

        Commands::Send { channel, message } => {
            println!("Wysylam wiadomosc do kanalu {}...", channel);
            let msg = client.send_message(&channel, &message).await?;
            println!("Wyslano! ID: {}\nTresc: {}", msg.id, msg.content);
        }

        Commands::Dm { user, message } => {
            println!("Otwieram DM z uzytkownikiem {}...", user);
            let dm = client.open_dm(&user).await?;
            println!("Kanal DM: {}", dm.id);
            let msg = client.send_message(&dm.id, &message).await?;
            println!("Wyslano! ID: {}\nTresc: {}", msg.id, msg.content);
        }

        Commands::Counter => {
            println!("Pobieram dane konta...");
            let me = client.get_me().await?;
            let display = me.global_name.clone().unwrap_or_else(|| me.username.clone());
            println!("Zalogowano jako: {} (ID: {})", display, me.id);
            run_counter(&client, &me.id).await?;
        }

        Commands::Weather => {
            println!("Pobieram dane konta...");
            let me = client.get_me().await?;
            let display = me.global_name.unwrap_or_else(|| me.username.clone());
            println!("Zalogowano jako: {}", display);
            run_weather(&client).await?;
        }

        Commands::LastLetter => {
            println!("Pobieram dane konta...");
            let me = client.get_me().await?;
            let display = me.global_name.clone().unwrap_or_else(|| me.username.clone());
            println!("Zalogowano jako: {} (ID: {})", display, me.id);
            run_last_letter(&client, &me.id).await?;
        }

        Commands::Stop => unreachable!(),
    }

    Ok(())
}
