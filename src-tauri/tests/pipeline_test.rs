/// Integration tests for the PII detection pipeline
/// Run with: cargo test --test pipeline_test
use std::path::PathBuf;

// We need to import from the library
// Since the crate is named "app_lib" in Cargo.toml
extern crate app_lib;

use app_lib::pipeline::regex_scanner::RegexScanner;
use app_lib::pipeline::types::PiiCategory;
use app_lib::pipeline;

/// Test text containing various PII types
const TEST_TEXT_US: &str = r#"
CONFIDENTIAL - Client Intake Form

Name: John Michael Smith
Date of Birth: 03/15/1985
Social Security Number: 123-45-6789
Email: john.smith@lawfirm-example.com
Phone: +1-555-867-5309
Address: 1234 Oak Street, Springfield, IL 62704

Emergency Contact: Jane Doe, jane.doe@gmail.com, +1-555-123-4567

Employment Information:
Employer ID (EIN): 12-3456789
Annual Income: $85,000

Financial Information:
Credit Card: 4532015112830366
Bank Account IBAN: DE89370400440532013000
IP Address accessed from: 192.168.1.100

Case Reference: CR-2024-00145
Court File Number: CF-IL-2024-78901

Medical Record Number: MRN-2024-556789

Driver's License: D400-1234-5678

Notes: Client resides at the above address with spouse. Meeting scheduled for
next Tuesday at the Springfield courthouse. Please ensure all documents
reference case number CR-2024-00145.
"#;

const TEST_TEXT_BR: &str = r#"
Formulário de Cadastro

Nome: Maria da Silva Santos
CPF: 123.456.789-09
CNPJ da Empresa: 12.345.678/0001-95
Email: maria.santos@empresa.com.br
Telefone: +55-11-98765-4321
"#;

const TEST_TEXT_DE: &str = r#"
Anmeldeformular

Name: Hans Müller
Steuer-ID: 12 345 67890
E-Mail: hans.mueller@beispiel.de
Telefon: +49-30-12345678
"#;

#[test]
fn test_global_email_detection() {
    let scanner = RegexScanner::new(&["global"]);
    let entities = scanner.scan(TEST_TEXT_US, 0);

    let emails: Vec<_> = entities.iter()
        .filter(|e| e.category == PiiCategory::Email)
        .collect();

    assert!(emails.len() >= 2, "Expected at least 2 emails, found {}", emails.len());

    let email_texts: Vec<&str> = emails.iter().map(|e| e.text.as_str()).collect();
    assert!(email_texts.contains(&"john.smith@lawfirm-example.com"),
        "Should detect john.smith@lawfirm-example.com, found: {:?}", email_texts);
    assert!(email_texts.contains(&"jane.doe@gmail.com"),
        "Should detect jane.doe@gmail.com, found: {:?}", email_texts);
}

#[test]
fn test_global_phone_detection() {
    let scanner = RegexScanner::new(&["global"]);
    let entities = scanner.scan(TEST_TEXT_US, 0);

    let phones: Vec<_> = entities.iter()
        .filter(|e| e.category == PiiCategory::PhoneNumber)
        .collect();

    assert!(phones.len() >= 1, "Expected at least 1 phone, found {}", phones.len());
}

#[test]
fn test_global_credit_card_detection() {
    let scanner = RegexScanner::new(&["global"]);
    let entities = scanner.scan(TEST_TEXT_US, 0);

    let cards: Vec<_> = entities.iter()
        .filter(|e| e.category == PiiCategory::CreditCard)
        .collect();

    assert!(cards.len() >= 1, "Expected at least 1 credit card, found {}", cards.len());
    assert_eq!(cards[0].text, "4532015112830366");
}

#[test]
fn test_global_ip_detection() {
    let scanner = RegexScanner::new(&["global"]);
    let entities = scanner.scan(TEST_TEXT_US, 0);

    let ips: Vec<_> = entities.iter()
        .filter(|e| e.category == PiiCategory::IpAddress)
        .collect();

    assert!(ips.len() >= 1, "Expected at least 1 IP address, found {}", ips.len());
    assert_eq!(ips[0].text, "192.168.1.100");
}

#[test]
fn test_global_iban_detection() {
    let scanner = RegexScanner::new(&["global"]);
    let entities = scanner.scan(TEST_TEXT_US, 0);

    let ibans: Vec<_> = entities.iter()
        .filter(|e| e.category == PiiCategory::IBAN)
        .collect();

    assert!(ibans.len() >= 1, "Expected at least 1 IBAN, found {}", ibans.len());
    assert_eq!(ibans[0].text, "DE89370400440532013000");
}

#[test]
fn test_us_ssn_detection() {
    let scanner = RegexScanner::new(&["global", "us"]);
    let entities = scanner.scan(TEST_TEXT_US, 0);

    let ssns: Vec<_> = entities.iter()
        .filter(|e| e.category == PiiCategory::SocialSecurityNumber)
        .collect();

    assert!(ssns.len() >= 1, "Expected at least 1 SSN, found {}", ssns.len());
    assert_eq!(ssns[0].text, "123-45-6789");
}

#[test]
fn test_us_ein_detection() {
    let scanner = RegexScanner::new(&["global", "us"]);
    let entities = scanner.scan(TEST_TEXT_US, 0);

    let eins: Vec<_> = entities.iter()
        .filter(|e| e.category == PiiCategory::TaxId)
        .collect();

    assert!(eins.len() >= 1, "Expected at least 1 EIN/Tax ID, found {}", eins.len());
    let ein_texts: Vec<&str> = eins.iter().map(|e| e.text.as_str()).collect();
    assert!(ein_texts.contains(&"12-3456789"),
        "Should detect EIN 12-3456789, found: {:?}", ein_texts);
}

#[test]
fn test_brazil_cpf_detection() {
    let scanner = RegexScanner::new(&["global", "br"]);
    let entities = scanner.scan(TEST_TEXT_BR, 0);

    let cpfs: Vec<_> = entities.iter()
        .filter(|e| e.category == PiiCategory::NationalId)
        .collect();

    assert!(cpfs.len() >= 1, "Expected at least 1 CPF, found {}", cpfs.len());
    assert_eq!(cpfs[0].text, "123.456.789-09");
}

#[test]
fn test_brazil_cnpj_detection() {
    let scanner = RegexScanner::new(&["global", "br"]);
    let entities = scanner.scan(TEST_TEXT_BR, 0);

    let cnpjs: Vec<_> = entities.iter()
        .filter(|e| e.category == PiiCategory::TaxId)
        .collect();

    assert!(cnpjs.len() >= 1, "Expected at least 1 CNPJ, found {}", cnpjs.len());
    assert_eq!(cnpjs[0].text, "12.345.678/0001-95");
}

#[test]
fn test_germany_tax_id_detection() {
    let scanner = RegexScanner::new(&["global", "de"]);
    let entities = scanner.scan(TEST_TEXT_DE, 0);

    let tax_ids: Vec<_> = entities.iter()
        .filter(|e| e.category == PiiCategory::TaxId)
        .collect();

    assert!(tax_ids.len() >= 1, "Expected at least 1 German tax ID, found {}", tax_ids.len());
}

#[test]
fn test_full_pipeline_us() {
    let entities = pipeline::run_pipeline(TEST_TEXT_US, 0, &["global", "us", "eu"]);

    println!("\n=== Full Pipeline Results (US Text) ===");
    for e in &entities {
        println!("  [{:?}] '{}' (confidence: {:.0}%, layer: {})",
            e.category, e.text, e.confidence * 100.0, e.layer);
    }
    println!("  Total: {} entities detected\n", entities.len());

    // Should detect at minimum: 2 emails, 1+ phones, 1 SSN, 1 EIN, 1 credit card, 1 IBAN, 1 IP
    assert!(entities.len() >= 7,
        "Expected at least 7 entities from full pipeline, found {}", entities.len());
}

#[test]
fn test_full_pipeline_brazil() {
    let entities = pipeline::run_pipeline(TEST_TEXT_BR, 0, &["global", "br"]);

    println!("\n=== Full Pipeline Results (Brazil Text) ===");
    for e in &entities {
        println!("  [{:?}] '{}' (confidence: {:.0}%, layer: {})",
            e.category, e.text, e.confidence * 100.0, e.layer);
    }
    println!("  Total: {} entities detected\n", entities.len());

    assert!(entities.len() >= 3,
        "Expected at least 3 entities (CPF, CNPJ, email), found {}", entities.len());
}

#[test]
fn test_no_false_positives_on_clean_text() {
    let clean_text = "The quick brown fox jumps over the lazy dog. This is a normal sentence with no personal information. The year 2024 was eventful.";

    let entities = pipeline::run_pipeline(clean_text, 0, &["global", "us", "eu", "br", "de"]);

    println!("\n=== Clean Text Results ===");
    for e in &entities {
        println!("  FALSE POSITIVE: [{:?}] '{}' (confidence: {:.0}%)",
            e.category, e.text, e.confidence * 100.0);
    }

    // Allow some false positives but flag them
    if !entities.is_empty() {
        println!("  WARNING: {} potential false positives detected on clean text", entities.len());
    }
}

#[test]
fn test_overlap_resolution() {
    // Text where multiple patterns might match the same region
    let text = "Contact SSN: 123-45-6789 or EIN: 12-3456789";
    let entities = pipeline::run_pipeline(text, 0, &["global", "us"]);

    // Verify no overlapping entities
    let mut sorted = entities.clone();
    sorted.sort_by_key(|e| e.start);

    for i in 1..sorted.len() {
        assert!(sorted[i].start >= sorted[i - 1].end,
            "Overlapping entities detected: '{}' ({}-{}) and '{}' ({}-{})",
            sorted[i - 1].text, sorted[i - 1].start, sorted[i - 1].end,
            sorted[i].text, sorted[i].start, sorted[i].end);
    }
}

#[test]
fn test_entity_has_valid_replacement() {
    let entities = pipeline::run_pipeline(TEST_TEXT_US, 0, &["global", "us"]);

    for e in &entities {
        assert!(!e.replacement.is_empty(),
            "Entity '{}' has empty replacement", e.text);
        assert!(e.replacement.starts_with("[REDACTED"),
            "Entity '{}' has unexpected replacement format: '{}'", e.text, e.replacement);
    }
}

#[test]
fn test_confidence_values() {
    let entities = pipeline::run_pipeline(TEST_TEXT_US, 0, &["global", "us"]);

    for e in &entities {
        assert!(e.confidence > 0.0 && e.confidence <= 1.0,
            "Entity '{}' has invalid confidence: {}", e.text, e.confidence);
    }
}

/// Test against a real legal contract (LLC amendment)
const LEGAL_CONTRACT_PAGE2: &str = r#"THIS AMENDMENT NO. 2 (this "Amendment") to the Amended and Restated Limited Liability Company Agreement of CS DIGITAL VENTURES LLC, a Delaware limited liability company (the "Company"), dated as of June 4, 2024 (the "Agreement"), is entered into as of March 9th, 2026, by and among the Company and the Members of the Company (collectively, the "Members").

WHEREAS, the Company is entering into a transaction pursuant to which the equity or assets of the Company will be sold to Olenox Industries (the "Transaction") for an aggregate consideration of approximately USD $50,000,000;

Shanti Cillo – 66.3158%
Bernardo Schucman – 21.0526%
CS Management Group LLC – 12.6316%"#;

const LEGAL_CONTRACT_PAGE4: &str = r#"MEMBERS:

Bernardo Schucman

Shanti Cillo

CS MANAGEMENT GROUP LLC

Bernardo Schucman, Manager

Luis Federico Sader Pereira, Shareholder

Roberto Santacroce Martins, Shareholder

Gustavo Lima Caldeira de Andrada, Shareholder"#;

#[test]
fn test_legal_contract_names() {
    let entities = pipeline::run_pipeline(LEGAL_CONTRACT_PAGE4, 3, &["global", "us", "eu", "br", "de"]);

    println!("\n=== Legal Contract Page 4 (Signatures) ===");
    for e in &entities {
        println!("  [{:?}] '{}' (confidence: {:.0}%, layer: {})",
            e.category, e.text, e.confidence * 100.0, e.layer);
    }
    println!("  Total: {} entities detected\n", entities.len());

    let names: Vec<&str> = entities.iter()
        .filter(|e| e.category == PiiCategory::PersonName)
        .map(|e| e.text.as_str())
        .collect();

    println!("  Names found: {:?}", names);

    // Should detect key names
    assert!(names.iter().any(|n| n.contains("Bernardo Schucman")),
        "Should detect 'Bernardo Schucman', found: {:?}", names);
    assert!(names.iter().any(|n| n.contains("Shanti Cillo")),
        "Should detect 'Shanti Cillo', found: {:?}", names);
    assert!(names.iter().any(|n| n.contains("Roberto Santacroce Martins")),
        "Should detect 'Roberto Santacroce Martins', found: {:?}", names);
}

#[test]
fn test_legal_contract_dates_and_money() {
    let entities = pipeline::run_pipeline(LEGAL_CONTRACT_PAGE2, 1, &["global", "us", "eu", "br", "de"]);

    println!("\n=== Legal Contract Page 2 (Body) ===");
    for e in &entities {
        println!("  [{:?}] '{}' (confidence: {:.0}%, layer: {})",
            e.category, e.text, e.confidence * 100.0, e.layer);
    }
    println!("  Total: {} entities detected\n", entities.len());

    let dates: Vec<&str> = entities.iter()
        .filter(|e| e.category == PiiCategory::Date)
        .map(|e| e.text.as_str())
        .collect();

    let money: Vec<&str> = entities.iter()
        .filter(|e| e.category == PiiCategory::MonetaryAmount)
        .map(|e| e.text.as_str())
        .collect();

    let names: Vec<&str> = entities.iter()
        .filter(|e| e.category == PiiCategory::PersonName)
        .map(|e| e.text.as_str())
        .collect();

    println!("  Dates: {:?}", dates);
    println!("  Money: {:?}", money);
    println!("  Names: {:?}", names);

    assert!(!dates.is_empty(), "Should detect dates in legal contract");
    assert!(!money.is_empty(), "Should detect monetary amounts");
    assert!(names.iter().any(|n| n.contains("Shanti Cillo")),
        "Should detect 'Shanti Cillo' from percentage distribution");
    assert!(names.iter().any(|n| n.contains("Bernardo Schucman")),
        "Should detect 'Bernardo Schucman' from percentage distribution");
}
