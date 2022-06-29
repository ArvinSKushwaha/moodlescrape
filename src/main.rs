use std::collections::HashMap;
use std::io::{stdin, stdout, BufRead, Write};
use std::path::PathBuf;
use std::time::Duration;

use owo_colors::colored::OwoColorize;
use thirtyfour::prelude::*;
use tokio;
use tokio::process::Command;

lazy_static::lazy_static! {
    static ref ICONMAP: HashMap<&'static str, bool> = {
        let mut m = HashMap::new();

        m.insert("https://moodle-courses2122.wolfware.ncsu.edu/theme/image.php/ncsu/core/1642858189/f/pdf-24", true);
        m.insert("https://moodle-courses2122.wolfware.ncsu.edu/theme/image.php/ncsu/folder/1642858189/icon", false); // TODO: Recurse into later
        m.insert("https://moodle-courses2122.wolfware.ncsu.edu/theme/image.php/ncsu/quiz/1642858189/icon", false);
        m.insert("https://moodle-courses2122.wolfware.ncsu.edu/theme/image.php/ncsu/url/1642858189/icon", false);
        m.insert("https://moodle-courses2122.wolfware.ncsu.edu/theme/image.php/ncsu/core/1642858189/f/powerpoint-24", true);
        m.insert("https://moodle-courses2122.wolfware.ncsu.edu/theme/image.php/ncsu/forum/1642858189/icon", false);
        m.insert("https://moodle-courses2122.wolfware.ncsu.edu/theme/image.php/ncsu/assign/1642858189/icon", false);
        m.insert("https://moodle-courses2122.wolfware.ncsu.edu/theme/image.php/ncsu/lti/1642858189/icon", false);
        m.insert("https://moodle-courses2122.wolfware.ncsu.edu/theme/image.php/ncsu/core/1642858189/f/html-24", false);

        m
    };
}

const DOWNLOAD_DIR: &str = "/home/arvinsk/Downloads/locations/";

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    // Query Moodle Url (if not in args)
    let url = match std::env::args().nth(1) {
        Some(url_arg) => url_arg,
        None => {
            let mut url = String::new();

            print!("{}", "Enter moodle link: ".yellow());

            stdout().lock().flush()?;
            stdin().lock().read_line(&mut url)?;
            url.trim().to_string()
        }
    };

    // Start by running the WebDriver.
    let cmd = Command::new("chromedriver")
        .arg("--port=4444")
        .kill_on_drop(true)
        .spawn()?;

    std::thread::sleep(Duration::from_secs(3));
    let mut caps = DesiredCapabilities::chrome();
    caps.add_chrome_option(
        "prefs",
        serde_json::json!({
            "download.default_directory": DOWNLOAD_DIR,
            "download.prompt_for_download": false,
            "download.directory_upgrade": true,
            "safebrowsing.enabled": true,
        }),
    )?;
    // caps.set_headless()?;

    let driver = WebDriver::new("http://localhost:4444/", caps).await?;

    println!("Url: {}", url);
    driver.get(url).await?;

    let ret = driver
        .execute_script(
            "return document.querySelectorAll(\".login\")[0].lastElementChild",
            Vec::new(),
        )
        .await?;

    ret.get_element()?.follow().await?;

    loop {
        let (usrnm, psswd) = {
            let mut usrnm = String::new();
            let psswd;

            print!("{}", "Enter username: ".yellow());

            stdout().lock().flush()?;
            stdin().lock().read_line(&mut usrnm)?;
            usrnm.pop();

            print!("{}", "Enter password: ".yellow());

            stdout().lock().flush()?;
            psswd = rpassword::read_password().unwrap();

            (usrnm, psswd)
        };

        let username_element = driver.find_element(By::Id("username")).await?;
        let password_element = driver.find_element(By::Id("password")).await?;
        let submit_element = driver.find_element(By::Id("formSubmit")).await?;

        driver
            .action_chain()
            .send_keys_to_element(&username_element, usrnm)
            .send_keys_to_element(&password_element, psswd)
            .click_element(&submit_element)
            .perform()
            .await?;

        break; // TODO: Add a proper login loop
    }

    driver
        .query(By::Id("dont-trust-browser-button"))
        .first()
        .await?
        .click()
        .await?;

    driver
        .query(By::Css(".btn.btn-red[value=Accept]"))
        .first()
        .await?
        .click()
        .await?;

    let courses = driver
        .query(By::Css(".aalink.coursename:not(.mr-2)"))
        .all()
        .await?;

    println!("Found courses:");
    for (i, course) in courses.iter().enumerate() {
        println!(
            "\t{}: {}",
            i,
            course.text().await?.trim_start_matches("Course name\n")
        );
    }

    let selected_course = loop {
        let mut course = String::new();

        print!("{}", "Choose course: ".yellow());

        stdout().lock().flush()?;
        stdin().lock().read_line(&mut course)?;

        course.pop();

        match course.parse::<usize>().ok() {
            Some(course_num) if course_num < courses.len() => break &courses[course_num],
            _ => println!("{}", "Couldn't parse, try again!".red()),
        }
    };

    selected_course.follow().await?;

    let links = driver.query(By::Css("a.aalink")).all().await?;
    let mut traverse = Vec::new();
    for link in links {
        let child = link.query(By::Css(":first-child")).first().await?;
        if *ICONMAP
            .get(&*child.get_property("src").await?.unwrap_or_default())
            .unwrap_or(&false)
        {
            if let Some(url) = link.get_property("href").await? {
                traverse.push(url);
            }
        }
    }

    for link in traverse {
        driver
            .in_new_tab(|| async {
                driver.get(link).await?;
                Ok(())
            })
            .await?;
    }

    // Busy wait for downloads to complete
    let download_dir: PathBuf = DOWNLOAD_DIR.into();
    let mut last_sizes;
    let mut sizes = None;
    loop {
        let mut curr_sizes = HashMap::<PathBuf, u64>::new();
        for path in download_dir.read_dir()? {
            let path = path?.path();
            curr_sizes.insert(path.clone(), path.metadata()?.len());
        }
        last_sizes = sizes.replace(curr_sizes);

        if last_sizes == sizes {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    driver.quit().await?;
    drop(cmd);
    Ok(())
}
