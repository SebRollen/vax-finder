use anyhow::Result;
use chrono::{DateTime, Utc};
use dotenv::dotenv;
use lettre::smtp::authentication::IntoCredentials;
use lettre::{SmtpClient, SmtpTransport, Transport};
use lettre_email::EmailBuilder;
use log::{info, trace};
use serde::Deserialize;
use std::env;
use tokio::time::interval;

#[derive(Debug, Deserialize)]
enum Area {
    Bronx,
    Brooklyn,
    Manhattan,
    Queens,
    #[serde(rename = "Staten Island")]
    StatenIsland,
    #[serde(rename = "Long Island")]
    LongIsland,
    #[serde(rename = "Mid-Hudson")]
    MidHudson,
}

impl std::fmt::Display for Area {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let string = match &self {
            Area::Bronx => "Bronx",
            Area::Brooklyn => "Brooklyn",
            Area::Manhattan => "Manhattan",
            Area::Queens => "Queens",
            Area::StatenIsland => "Staten Island",
            Area::LongIsland => "Long Island",
            Area::MidHudson => "Mid-Hudson",
        };
        write!(f, "{}", string)
    }
}

#[derive(Debug, Deserialize)]
struct Appointments {
    count: usize,
    summary: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Location {
    active: bool,
    appointments: Appointments,
    area: Area,
    available: bool,
    id: String,
    last_available_at: Option<DateTime<Utc>>,
    name: String,
    portal: String,
    updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PortalType {
    Clinic,
    Government,
    Pharmacy,
}

#[derive(Debug, Deserialize)]
struct Portal {
    key: String,
    name: String,
    url: String,
    #[serde(rename = "type")]
    portal_type: PortalType,
}

#[derive(Debug, Deserialize)]
struct Response {
    last_updated_at: DateTime<Utc>,
    locations: Vec<Location>,
    portals: Vec<Portal>,
}

struct Email {
    email: String,
    mailer: SmtpTransport,
}

impl Email {
    fn new(email: &str, password: &str) -> Result<Self> {
        let credentials = (email, password).into_credentials();
        let mailer: SmtpTransport = SmtpClient::new_simple("smtp.gmail.com")?
            .credentials(credentials)
            .transport();
        Ok(Self {
            email: email.to_string(),
            mailer,
        })
    }

    fn notify(&mut self, location: &Location, portal: Option<&Portal>) -> Result<()> {
        let mut body = format!(
            "Found {} vaccine appoitnemnt(s)! The location is {} in {}.\n",
            location.appointments.count, location.name, location.area
        );
        if let Some(details) = &location.appointments.summary {
            body.push_str(details);
            body.push_str("\n");
        }
        if let Some(portal) = portal {
            body.push_str(&format!(
                "Appointments can be booked through the {} portal, at {}",
                portal.name, portal.url
            ));
        } else {
            body.push_str("Visit turbovax.info for more information");
        }
        let email = EmailBuilder::new()
            .from(self.email.as_str())
            .to(self.email.as_str())
            .subject("Vaccine slot found!")
            .body(body)
            .build()?
            .into();
        self.mailer.send(email).map(|_| ()).map_err(From::from)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv()?;
    env_logger::init();

    let mut client = Email::new(&env::var("LETTRE_EMAIL")?, &env::var("LETTRE_PASSWORD")?)?;
    let mut interval = interval(std::time::Duration::from_secs(60));

    loop {
        interval.tick().await;
        trace!("Evaluating");
        let res = reqwest::get("https://turbovax.global.ssl.fastly.net/dashboard").await?;
        let res: Response = serde_json::from_str(&res.text().await?)?;
        let locations = res.locations.iter().filter_map(|location| {
            if location.available {
                Some(location)
            } else {
                None
            }
        });
        for location in locations {
            let portal = res
                .portals
                .iter()
                .find(|portal| portal.key == location.portal);
            info!(
                "Appointment found. Location: {:?}, portal: {:?}",
                location, portal
            );
            client.notify(location, portal)?;
        }
    }
}
