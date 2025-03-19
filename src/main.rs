use std::cell::Cell;
use std::error::Error;
use std::time::{Duration, Instant};
use std::thread::sleep;

use reqwest::blocking::Client;
use reqwest::header::USER_AGENT;

use serde::Deserialize;
use serde_json::Value;
use serde_xml_rs::from_str;

use scraper::{Html, Selector};


/// Root element of the SEC Atom XML response.
#[derive(Debug, Deserialize)]
#[serde(rename = "feed")]
struct Feed {
    #[serde(rename = "entry", default)]
    entries: Vec<Entry>,
}

/// Individual filing entry.
#[derive(Debug, Deserialize)]
struct Entry {
    #[serde(rename = "link")]
    link: Link,
}

/// Represents link to a specific filing.
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
pub fn documents(sec_client: &SecClient, ticker: &str, date: &str)
                 -> Result<Vec<String>, Box<dyn Error>> {

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
    for document in &documents_list[0..1] {

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
            // if not found, simply jump to next doc
            None => {
                continue;
            }
        };
    }

    Ok(summaries)
}



/// Root element of the SEC XML response.
#[derive(Debug, Deserialize)]
struct FilingSummary {
    #[serde(rename = "MyReports")]
    filing: Reports,
}

/// Represents the `<MyReports>` section in the XML.
#[derive(Debug, Deserialize)]
#[serde(rename = "MyReports")]
struct Reports {
    #[serde(rename = "Report", default)]
    reports: Vec<Report>,
}

/// Individual report entries inside `<MyReports>`.
#[derive(Debug, Deserialize)]
struct Report {
    #[serde(rename = "ShortName")]
    shortname: Option<String>,
    #[serde(rename = "HtmlFileName")]
    htmlfilename: Option<String>,
    #[serde(rename = "XmlFileName")]
    xmlfilename: Option<String>,
}


fn master_reports(sec_client: &SecClient, xml_summaries: &[String])
                  -> Result<Vec<(String, String)>, Box<dyn Error>> {

    let mut all_reports = vec![];

    for xml_url in xml_summaries {
        // GET request
        let xml_content = sec_client.get(xml_url)?;

        // get base URL
        let base_url = xml_url.replace("FilingSummary.xml", "");

        // deserialize filing elements
        let xml_summary: FilingSummary = from_str(&xml_content)?;

        // extract reports from the XML summary
        let mut reports = xml_summary.filing.reports;

        // exclude the last report which should aways be the 'base_url'
        if reports.len() > 1 {
            reports.pop();
        }

        // process each report
        for report in reports {
            // prefer htmlfilename over xmlfilename
            let file = report.htmlfilename.or(report.xmlfilename).unwrap_or_default();

            // grab url and its short description
            let url = format!("{}{}", base_url, file);
            let shortname = report.shortname.unwrap_or_default();
            all_reports.push((shortname, url));
        }
    }
    Ok(all_reports)
}



/// Struct to hold the parsed table data.
pub struct StatementData {
    pub headers: Vec<Vec<String>>,
    pub sections: Vec<String>,
    pub data: Vec<Vec<String>>,
}



/// Parses HTML content of a SEC filing page, extract statement
/// data.
pub fn parse_html_statement_data(html: &str) -> StatementData {

    let mut statement_data = StatementData {
        headers: Vec::new(),
        sections: Vec::new(),
        data: Vec::new(),
    };


    // parse html
    let document = Html::parse_document(html);
    let table_selector = Selector::parse("table").expect("Failed to parse 'table' tag");
    let tr_selector = Selector::parse("tr").expect("Failed to parse 'tr' tag");
    let th_selector = Selector::parse("th").expect("Failed to parse 'th' tag");
    let td_selector = Selector::parse("td").expect("Failed to parse 'td' tag");
    let strong_selector = Selector::parse("strong").expect("Failed to parse 'strong' tag");


    // find the first table element
    if let Some(table) = document.select(&table_selector).next() {
        for tr in table.select(&tr_selector) {
            let ths: Vec<_> = tr.select(&th_selector).collect();
            let tds: Vec<_> = tr.select(&td_selector).collect();
            let strongs: Vec<_> = tr.select(&strong_selector).collect();

            // document header
            if !ths.is_empty() {
                let header_row = ths.iter().map(|col| col.text()
                                 .collect::<Vec<_>>().join(" ").trim()
                                 .to_string()).collect();
                statement_data.headers.push(header_row);
            }

            // document section row (under header)
            else if !tds.is_empty() && !strongs.is_empty() {
                let section_row = tds[0].text().collect::<Vec<_>>().join(" ")
                                  .trim().to_string();
                statement_data.sections.push(section_row);
            }

            // data rows (under section)
            else if !tds.is_empty() && strongs.is_empty() {
                let data_row: Vec<String> = tds.iter().map(|col| col.text()
                                            .collect::<Vec<_>>().join(" ").trim()
                                            .to_string()).collect();
                statement_data.data.push(data_row);
            }

            else {
                println!("\nERROR: Unrecognized HTML structure in a <tr>.\n");
            }
        }
    }

    else {
        println!("\nERROR: No <table> found in the HTML.\n");
    }

    return statement_data;
}



pub fn balance_sheets(sec_client: &SecClient, xml_summaries:
                      &Vec<(String, String)>) -> Result<StatementData, Box<dyn Error>> {

    let keywords = ["balance sheets", "financial condition"];


    // find the shortname == keywords, and parse its url
    for (name, url) in xml_summaries.iter() {
        if keywords.iter().any(|&kw| name.to_lowercase().contains(kw)) {

            println!("{}", url);

            // GET html
            let html = sec_client.get(&url)?;

            // parse html
            let statement_data = parse_html_statement_data(&html);

            return Ok(statement_data);
        }
    }

    return Err(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Could not find balance sheet url.",
    )));
}




fn main() {

    let sec_client = SecClient::new().expect("Failed to create client");

    let docs = documents(&sec_client, "aapl", "").unwrap();

    /*
    println!("\ndocuments:");
    for d in &docs {
        println!("{}", d);
    }
    */

    let filings = filing_summaries(&sec_client, &docs).unwrap();

    /*
    println!("\nfiling summary:");
    for f in &filings {
        println!("{}", f);
    }
    */

    let reports = master_reports(&sec_client, &filings).unwrap();

    println!("\nmaster reports:");
    for r in &reports {
        let (desc, url) = r;
        println!("{}:", desc);
        println!("{}\n", url);
    }


    let bs = balance_sheets(&sec_client, &reports).unwrap();

    println!("\n{:?}", bs.headers);
    println!("\n{:?}", bs.sections);
    println!("\n{:?}", bs.data);

}
