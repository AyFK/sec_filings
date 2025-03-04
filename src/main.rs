
use reqwest::blocking::Client;
use reqwest::header::USER_AGENT;
use serde_xml_rs::from_str;
use serde::Deserialize;
use std::error::Error;



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
}

//let header = "MyOrgName/1.0 (my.email@address.com) - For academic or research purposes";
//let header = String::from("Name/1.0 (my.email@address.com) - Research");
impl SecClient {
    pub fn new() -> Result<Self, reqwest::Error> {
        let header = String::from("my.email@address.com");
        let client = Client::builder().build()?;
        return Ok(SecClient { client, header });
    }

    // GET request from basic URL
    pub fn get(&self, url: &str) -> Result<String, reqwest::Error> {
        return self.client.get(url).header(USER_AGENT,
               self.header.as_str()).send()?.text();
    }

    // GET request from URL with query parameters
    pub fn get_with_params(&self, url: &str, params: &[(&str, &str)]) ->
                           Result<String, reqwest::Error> {

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
        //println!("{:?}", entry.link.href);

        let mut documents_url = entry.link.href
            .replace("-index.html", "/index.json")
            .replace("-index.htm", "/index.json")
            .replace("-", "");

        //println!("{:?}", documents_url);

        // adjust the URL if it has exactly 10 parts
        let split_items: Vec<&str> = documents_url.split("/").collect();
        if split_items.len() == 10 {
            let mut items = split_items;
            items.remove(7);
            documents_url = items.join("/");
        }

        //println!("{:?}", documents_url);
        documents_list.push(documents_url);
    }
    Ok(documents_list)
}








fn main() {

    let sec_client = SecClient::new()
                                .expect("Failed to create client");

    let docs = documents(&sec_client, "aapl", "").unwrap();

    for d in docs {
        println!("{}", d);
    }


}
