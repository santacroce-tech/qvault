use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PiiCategory {
    PersonName,
    Email,
    PhoneNumber,
    Address,
    DateOfBirth,
    Date,
    MonetaryAmount,
    SocialSecurityNumber,
    NationalId,
    PassportNumber,
    DriversLicense,
    BankAccount,
    CreditCard,
    IBAN,
    TaxId,
    CaseNumber,
    CourtFileNumber,
    ClientId,
    IpAddress,
    Url,
    LicensePlate,
    MedicalRecordNumber,
    CompanyName,
}

impl PiiCategory {
    pub fn replacement_label(&self) -> &'static str {
        match self {
            PiiCategory::PersonName => "[REDACTED NAME]",
            PiiCategory::Email => "[REDACTED EMAIL]",
            PiiCategory::PhoneNumber => "[REDACTED PHONE]",
            PiiCategory::Address => "[REDACTED ADDRESS]",
            PiiCategory::DateOfBirth => "[REDACTED DOB]",
            PiiCategory::Date => "[REDACTED DATE]",
            PiiCategory::MonetaryAmount => "[REDACTED AMOUNT]",
            PiiCategory::SocialSecurityNumber => "[REDACTED SSN]",
            PiiCategory::NationalId => "[REDACTED ID]",
            PiiCategory::PassportNumber => "[REDACTED PASSPORT]",
            PiiCategory::DriversLicense => "[REDACTED LICENSE]",
            PiiCategory::BankAccount => "[REDACTED ACCOUNT]",
            PiiCategory::CreditCard => "[REDACTED CARD]",
            PiiCategory::IBAN => "[REDACTED IBAN]",
            PiiCategory::TaxId => "[REDACTED TAX ID]",
            PiiCategory::CaseNumber => "[REDACTED CASE]",
            PiiCategory::CourtFileNumber => "[REDACTED COURT FILE]",
            PiiCategory::ClientId => "[REDACTED CLIENT ID]",
            PiiCategory::IpAddress => "[REDACTED IP]",
            PiiCategory::Url => "[REDACTED URL]",
            PiiCategory::LicensePlate => "[REDACTED PLATE]",
            PiiCategory::MedicalRecordNumber => "[REDACTED MRN]",
            PiiCategory::CompanyName => "[REDACTED COMPANY]",
        }
    }

    pub fn to_string(&self) -> String {
        serde_json::to_value(self)
            .unwrap()
            .as_str()
            .unwrap()
            .to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiEntity {
    pub id: String,
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub page: u32,
    pub category: PiiCategory,
    pub layer: String,
    pub confidence: f64,
    pub replacement: String,
}
