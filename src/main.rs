use std::collections::HashMap;

use reqwest::blocking::Client;
use reqwest::header::USER_AGENT;


/// Fetch {ticker : cik} key-value pairs from SEC website.
pub fn list() -> Result<HashMap<String, u64>, reqwest::Error> {

    // URL of interest
    let url = "https://www.sec.gov/include/ticker.txt";

    // build a client
    let client = Client::builder().build()?;

    // valid User-Agent header as recommended by the SEC
    let header = "MyOrgName/1.0 (my.email@address.com) - For academic or research purposes";
    let response = client.get(url).header(USER_AGENT, header).send()?.text();

    // store and return pairs in this map
    let mut map = HashMap::new();

    // see if we reached URL
    match response {

        Ok(body) => {
            // split body into rows
            let rows = body.split("\n").collect::<Vec<&str>>();

            // iterate over each row
            for row in rows {

                // split row into columns
                let ticker_cik: Vec<&str> = row.split("\t").collect();

                // tell-tale sign of a User-Agent error
                if ticker_cik.len() < 2 {
                    panic!("ERROR: Exceeded request rate threshold on {}. \
                            User-Agent header expired.", &url);
                }

                // if SEC lets us in, add key-value pair
                else if let Ok(cik) = ticker_cik[1].parse::<u64>() {
                    map.insert(ticker_cik[0].to_string(), cik);
                }
            }

            return Ok(map);
        },

        Err(e) => {
            panic!("ERROR: Could not reach: {}. {}.", &url, &e);
        },
    }
}


/// Given a {ticker : cik} key-value pairs, return the cik,
/// with 10 valid characters.
pub fn cik(map: &HashMap<String, u64>, ticker: &str) -> Option<String> {
    match map.get(&ticker.to_lowercase()) {
        Some(cik) => {
            return Some(format!("{:010}", *cik));
        },
        None => {
            return None;
        },
    }
}



fn main() {
    let map = list().unwrap();

    /*
    for (key, value) in &map {
        println!("Key: {}, Value: {}", key, value);
    }
    */

    let cik = cik(&map, "aapl").unwrap_or(String::from("None"));
    println!("{}", cik);
}
