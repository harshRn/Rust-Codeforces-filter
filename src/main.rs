#[macro_use] extern crate rocket;
extern crate reqwest;
extern crate futures;
extern crate tokio;
extern crate scraper;

use tokio::runtime::Runtime;
use scraper::{Html, Selector};
use rocket::time::Date;
use rocket::http::{Status, ContentType};
use rocket::form::{Form, Contextual, FromForm, FromFormField, Context};
use rocket::fs::{FileServer, TempFile, relative};
use rocket_dyn_templates::Template;
use rocket_dyn_templates::context;
use futures::executor::block_on;
use serde::Serialize;

use std::{thread, time};
use std::sync::{Arc,Mutex};
use std::collections::HashMap;

#[get("/")]
fn index() -> Template {
    Template::render("index", &Context::default())
}


#[derive(Debug, FromForm)]
struct Query<'v> {
    link: &'v str,
    ll:   &'v str,
    ul:   &'v str,
}

#[derive(Debug, FromForm)]
struct Submit<'v> {
    query: Query<'v>,
}

#[derive(Serialize)]
struct Problem{
    rating   : String,
    link     : String,
}

#[post("/", data = "<form>")]
fn submit<'r>(form: Form<Contextual<'r, Submit<'r>>>) -> (Status, Template) {
               let template = match form.value {
        Some(ref submission) => {
            let fetched: Arc<Mutex<Vec<Vec<String>>>> = Arc::new(Mutex::new(Vec::new()));
            let limits = process(submission, Arc::clone(&fetched));
            let results: HashMap<String, Problem> = post_processing(fetched, limits[0], limits[1]);
            Template::render("success", context!{results})
        }
        None => {
            Template::render("index", &form.context)
        }
    };

    (form.context.status(), template)
}

fn post_processing(fetched: Arc<Mutex<Vec<Vec<String>>>>, ll: u32, ul: u32) -> HashMap<String,Problem> {
    let mut stored = fetched.lock().unwrap();
    let mut counter: u32 = 0;
    let mut results: HashMap<String, Problem> = HashMap::new();
    for item in &*stored {
        let anchor_selector = Selector::parse("a").unwrap();
        let span_selector   = Selector::parse("span").unwrap();

        let h_code   = Html::parse_fragment(&item[0]);
        let h_rating = Html::parse_fragment(&item[1]);

        let p_code   = h_code.select(&anchor_selector).next().unwrap();
        let p_rating = h_rating.select(&span_selector).next().unwrap();

        let code = p_code.text().collect::<String>();
        let code = code.trim();
        let rating = p_rating.text().collect::<String>();
        let rating = rating.trim();
        let n_rating = rating.to_string().parse::<u32>().unwrap();

        let code_alpha = code.chars().last().unwrap();
        let mut code_numeric = code.chars();
        code_numeric.next_back();
        let code_numeric = code_numeric.as_str();

        let mut link = "https://codeforces.com/problemset/problem/".to_owned();
        let section  = format!("{}/{}", code_numeric, code_alpha);
        link.push_str(&section);

        if n_rating >= ll && n_rating <= ul {
            results.insert(code.to_string(), Problem {
                rating : rating.to_string(),
                link,
            });
        }
    }
    return results;
}

fn process<'r>(values: &Submit<'r>, fetched: Arc<Mutex<Vec<Vec<String>>>>) -> Vec<u32> {
        let link = values.query.link.trim().to_string();
        let ill  = values.query.ll.trim().to_string().parse::<u32>().unwrap();
        let iul  = values.query.ul.trim().to_string().parse::<u32>().unwrap();

//         https://users.rust-lang.org/t/how-to-use-async-fn-in-thread-spawn/46413/3
        thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            let future = fetchProblems(link.clone(), Arc::clone(&fetched));
            let res = rt.block_on(future);
        }).join().unwrap();

        let h_millis = time::Duration::from_millis(50);

        let parsed = vec![ill, iul];

        thread::sleep(h_millis);
        return parsed;

}

async fn fetchProblems<'r>(link: String, store: Arc<Mutex<Vec<Vec<String>>>>) -> Result<(), Box<dyn std::error::Error>> {
    let mut storage = store.lock().unwrap();

    // Request the page with a GET request
    let resp = reqwest::get(link).await?.text().await?;

    let document = Html::parse_document(&resp);
    let tableSelector = Selector::parse(r#"table[class="problems"]"#).unwrap();
    let rowSelector = Selector::parse("tr").unwrap();
    let dataSelector = Selector::parse("td").unwrap();
    let linkSelector = Selector::parse("a").unwrap();
    let ratingSelector = Selector::parse(r#"span[title="Difficulty"]"#).unwrap();
    let mainTable = document.select(&tableSelector).next().unwrap();

    //holds each row with relevant info
    let mut s_rating = String::new();
    for row in mainTable.select(&rowSelector) {
        let mut single = vec![];
        for td in row.select(&dataSelector) {
            for link in td.select(&linkSelector) {
                single.push(link.html());
            }
            //don't need this lookup if the selector can be generalized
            let rating = td.select(&ratingSelector).next();
            s_rating = match rating {
                Some(x) => x.html(),
                None    => "".to_string(),
            };
            if s_rating.len() != 0 {
                single.push(s_rating);
            }


            // TODO : remove remove
            //discarding fields that are not required : inefficient
            while single.len() > 4 {
                single.remove(2);
            }
            // single.remove(single.len()-1);
        }
        //removing name and no. of successful submissions
        if single.len() ==4 {
            single.remove(1);
            single.remove(2);
        }
        (*storage).push(single);
    }
    (*storage).remove(0);

    // Parse the response body as HTML
       Ok(())
}



#[launch]
fn rocket() -> _ {
    rocket::build()
        .mount("/", routes![index, submit])
        .attach(Template::fairing())
}
