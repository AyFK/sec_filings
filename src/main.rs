use std::cell::Cell;
use std::error::Error;
use std::time::{Duration, Instant};
use std::thread::sleep;

use reqwest::blocking::Client;
use reqwest::header::USER_AGENT;

use serde::Deserialize;
use serde_xml_rs::from_str;
use serde_json::Value;




/// structs to deserialize the SEC Atom XML response
#[derive(Debug, Deserialize)]
#[serde(rename = "feed")]
struct Feed {
    #[serde(rename = "entry", default)]
    entries: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    #[serde(rename = "link")]
    link: Link,
}

#[derive(Debug, Deserialize)]
struct Link {
    #[serde(rename = "href")]
    href: String,
}


pub struct SecClient {
    client: Client,
    header: String,

    time_start: Cell<Instant>,
    request_count: Cell<u8>,
    request_threshold: u8
}


impl SecClient {

    pub fn new() -> Result<Self, reqwest::Error> {
        // required for successful access
        let header = String::from("my.email@address.com");
        let client = Client::builder().build()?;

        // required to resepct threshold of 10 requests per second
        // replace cell with mutex to make it multi-core later on
        let time_start = Cell::new(Instant::now());
        let request_count = Cell::new(0);
        let request_threshold = 10;

        let instance = Self {
            client,
            header,
            time_start,
            request_count,
            request_threshold,
        };

        return Ok(instance);
    }


    fn threshold_reset(&self) {
            self.time_start.set(Instant::now());
            self.request_count.set(0);
    }


    fn threshold_status(&self) {
        let time_now = Instant::now();
        let time_elapsed = time_now.duration_since(self.time_start.get());

        // if more than a second has past, reset 'request_count'
        if time_elapsed >= Duration::from_secs(1) {
            self.threshold_reset();
        }


        // if we have reached threshold
        if self.request_count.get() >= self.request_threshold {
            let sleep_needed = Duration::from_secs(1)
                                        .saturating_sub(time_elapsed);
            sleep(sleep_needed);
            self.threshold_reset();
        }

        // increment request count
        self.request_count.set(self.request_count.get() + 1);
    }



    // GET request from basic URL
    pub fn get(&self, url: &str) -> Result<String, reqwest::Error> {

        self.threshold_status();

        return self.client.get(url).header(USER_AGENT,
               self.header.as_str()).send()?.text();
    }

    // GET request from URL with query parameters
    pub fn get_with_params(&self, url: &str, params: &[(&str, &str)]) ->
                           Result<String, reqwest::Error> {

        self.threshold_status();

        return self.client.get(url).query(params).header(USER_AGENT,
               self.header.as_str()).send()?.text();
    }
}




/// Get document URLs for a given ticker and date
pub fn documents(sec_client: &SecClient, ticker: &str, date: &str) -> Result<Vec<String>, Box<dyn Error>> {

    let params = [
        ("action", "getcompany"),
        ("ticker", ticker),
        ("type", "10-Q"),
        ("dateb", date),
        ("owner", "exclude"),
        ("start", ""),
        ("output", "atom"),
        ("count", "100"),
    ];

    let base_url = "https://www.sec.gov/cgi-bin/browse-edgar";
    let response = sec_client.get_with_params(base_url, &params)?;

    // deserialize the Atom feed XML into 'Feed' struct
    let feed: Feed = from_str(&response)?;

    let mut documents_list = Vec::new();

    for entry in feed.entries {

        let mut documents_url = entry.link.href
            .replace("-index.html", "/index.json")
            .replace("-index.htm", "/index.json")
            .replace("-", "");

        // adjust the URL if it has exactly 10 parts
        let split_items: Vec<&str> = documents_url.split("/").collect();
        if split_items.len() == 10 {
            let mut items = split_items;
            items.remove(7);
            documents_url = items.join("/");
        }

        documents_list.push(documents_url);
    }
    Ok(documents_list)
}



fn filing_summaries(sec_client: &SecClient, documents_list: &Vec<String>)
                     -> Result<Vec<String>, Box<dyn Error>> {

    // store all summeries from documents_list in here
    let mut summaries = vec![];

    // iterate over each JSON index URL
    for document in documents_list {

        // GET request
        let response = sec_client.get(&document).unwrap();

        // parse the JSON content
        let json_data: Value = serde_json::from_str(&response)?;

        // In the JSON, find the directory and its items.
        let directory = &json_data["directory"];
        let dir_name = directory["name"].as_str().ok_or(
                       "Directory name not found in JSON")?;

        let items = directory["item"].as_array().ok_or(
                    "No items found in directory")?;

        // look for "FilingSummary.xml" in the directory items
        let base_url = "https://www.sec.gov";
        let mut xml_summary_url = None;

        for item in items {
            if item["name"].as_str() == Some("FilingSummary.xml") {
                // construct the URL for the FilingSummary.xml
                xml_summary_url = Some(format!("{}/{}/{}", base_url,
                                    dir_name, "FilingSummary.xml"));
                break;
            }
        }

        match xml_summary_url {
            // found it
            Some(url) => {
                summaries.push(url);
            }
            // if not found, simply jump to next doc list
            None => {
                continue;
            }
        };
    }

    Ok(summaries)
}



fn main() {

    let sec_client = SecClient::new()
                                .expect("Failed to create client");

    let docs = documents(&sec_client, "aapl", "").unwrap();

    for d in &docs {
        println!("{}", d);
    }

    let filings = filing_summaries(&sec_client, &docs).unwrap();

    for f in &filings {
        println!("{}", f);
    }
}
