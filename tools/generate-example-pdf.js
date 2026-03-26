import PDFDocument from "pdfkit";
import { createWriteStream } from "fs";
import { fileURLToPath } from "url";
import { dirname, join } from "path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const outputPath = join(__dirname, "example-contract.pdf");

const doc = new PDFDocument({
  size: "letter",
  margins: { top: 72, bottom: 72, left: 72, right: 72 },
});

const stream = createWriteStream(outputPath);
doc.pipe(stream);

// ── Helpers ──────────────────────────────────────────────────────────────────

function title(text) {
  doc.fontSize(16).font("Helvetica-Bold").text(text, { align: "center" });
  doc.moveDown(0.3);
}

function sectionHeading(num, text) {
  doc.moveDown(0.6);
  doc.fontSize(11).font("Helvetica-Bold").text(`${num}. ${text.toUpperCase()}`);
  doc.moveDown(0.3);
}

function body(text) {
  doc.fontSize(10).font("Helvetica").text(text, { align: "justify", lineGap: 2 });
  doc.moveDown(0.3);
}

function signatureLine(name, titleRole, company, date) {
  doc.moveDown(0.8);
  doc.fontSize(10).font("Helvetica").text("_________________________________________");
  doc.font("Helvetica-Bold").text(name);
  doc.font("Helvetica").text(`${titleRole}, ${company}`);
  doc.text(`Date: ${date}`);
}

// ── Document Content ─────────────────────────────────────────────────────────

title("PROFESSIONAL SERVICES AGREEMENT");
doc.fontSize(10).font("Helvetica").text("Contract No. PSA-2024-00847", { align: "center" });
doc.moveDown(0.2);
doc.text("Effective Date: June 15, 2024", { align: "center" });
doc.moveDown(1);

// Section 1 — Parties
sectionHeading("1", "Parties");
body(
  'This Professional Services Agreement ("Agreement") is entered into as of June 15, 2024, ' +
    "by and between:"
);
body(
  "Meridian Consulting Group LLC, a limited liability company organized under the laws of the " +
    "State of California, with its principal place of business at 1420 Market Street, Suite 200, " +
    'San Francisco, CA 94103 (hereinafter referred to as "Consultant"), represented by ' +
    "Maria Santos, Managing Partner, email: maria.santos@meridian-consulting.com, " +
    "phone: (415) 555-0187;"
);
body("and");
body(
  "Atlas Digital Solutions Inc, a corporation organized under the laws of the State of New York, " +
    "with its principal place of business at 350 Fifth Avenue, Floor 12, New York, NY 10118 " +
    '(hereinafter referred to as "Client"), represented by James Richardson, Chief Executive Officer, ' +
    "email: j.richardson@atlas-digital.com, phone: (212) 555-0234."
);

// Section 2 — Term
sectionHeading("2", "Term and Duration");
body(
  "This Agreement shall commence on June 15, 2024 and shall remain in effect through " +
    "December 31, 2025, unless terminated earlier in accordance with the provisions of Section 8. " +
    "Either party may request a renewal in writing no later than thirty (30) days prior to the " +
    "expiration date. A subsequent renewal term beginning March 1, 2026 may be negotiated at " +
    "the mutual agreement of both parties."
);

// Section 3 — Scope
sectionHeading("3", "Scope of Services");
body(
  "Consultant agrees to provide the following professional services to Client: strategic technology " +
    "consulting, software architecture review, data migration planning, cloud infrastructure assessment, " +
    "and staff augmentation for the Client's enterprise modernization initiative (the \"Project\"). " +
    "The Project Manager on behalf of Consultant shall be Elena Kowalski, Senior Director of " +
    "Technology Services (elena.kowalski@meridian-consulting.com). The Client's designated point of " +
    "contact shall be Roberto Fernandes, VP of Engineering (r.fernandes@atlas-digital.com)."
);
body(
  "Consultant shall provide a team of no fewer than five (5) qualified professionals to support " +
    "the Project. All personnel assigned to the Project shall possess relevant industry certifications " +
    "and a minimum of five (5) years of professional experience in the applicable domain."
);

// Section 4 — Compensation
sectionHeading("4", "Compensation");
body(
  "In consideration for the services described herein, Client agrees to pay Consultant a total fee " +
    "not to exceed One Hundred Fifty Thousand Dollars ($150,000.00) for the initial term of this " +
    "Agreement. The compensation shall be structured as follows:"
);
body(
  "(a) An initial retainer of Eighty-Five Thousand Dollars ($85,000.00) shall be payable within " +
    "fifteen (15) business days of the execution of this Agreement."
);
body(
  "(b) A milestone payment of Fifty Thousand Dollars ($50,000.00) shall be due upon completion " +
    "and Client acceptance of the Phase 1 deliverables, as defined in Exhibit A."
);
body(
  "(c) The remaining balance of Fifteen Thousand Dollars ($15,000.00) shall be payable upon " +
    "final delivery and acceptance of all Project deliverables."
);

// Section 5 — Payment Terms
sectionHeading("5", "Payment Terms and Banking Information");
body(
  "All payments shall be made via wire transfer to the following account designated by Consultant:"
);
body(
  "Bank: First National Bank of California\n" +
    "Account Holder: Meridian Consulting Group LLC\n" +
    "Account Number: 7823401956\n" +
    "Routing Number: 021000021\n" +
    "SWIFT Code: FNBCUS6S"
);
body(
  "Invoices shall be submitted monthly and are due net thirty (30) days from the date of receipt. " +
    "Late payments shall accrue interest at a rate of 1.5% per month or the maximum rate permitted " +
    "by applicable law, whichever is less."
);

// Section 6 — Expense Reimbursement
sectionHeading("6", "Expense Reimbursement");
body(
  "Client shall reimburse Consultant for all reasonable and pre-approved travel and business expenses " +
    "incurred in connection with the performance of services under this Agreement. Expenses exceeding " +
    "Five Hundred Dollars ($500.00) require prior written approval from the Client."
);
body(
  "For recurring project-related expenses, Consultant has designated the following corporate credit " +
    "card for expense tracking and reconciliation purposes: Visa ending in 6789, full number " +
    "4532-8901-2345-6789, held by Maria Santos. All expense reports shall be submitted with " +
    "supporting receipts within fifteen (15) days of the expense being incurred."
);

// Section 7 — Tax Information
sectionHeading("7", "Tax Information and Compliance");
body(
  "Consultant represents and warrants that it is an independent contractor and not an employee of " +
    "Client for tax purposes. Consultant's Employer Identification Number (EIN) is 82-4751093. " +
    "Consultant shall be solely responsible for the payment of all federal, state, and local taxes " +
    "arising from compensation received under this Agreement."
);
body(
  "For purposes of tax reporting and W-9 compliance, the following information is provided for the " +
    "principal signatory: Maria Santos, Social Security Number: 412-68-9103, " +
    "Date of Birth: March 15, 1985. This information shall be kept strictly confidential and used " +
    "solely for tax reporting obligations as required by the Internal Revenue Service."
);

// Section 8 — Confidentiality
sectionHeading("8", "Confidentiality");
body(
  "Each party acknowledges that during the course of this Agreement, it may receive or have access to " +
    "confidential and proprietary information belonging to the other party, including but not limited to " +
    "trade secrets, business plans, financial data, customer lists, technical specifications, and " +
    'personnel records (collectively, "Confidential Information"). Each party agrees to hold all ' +
    "Confidential Information in strict confidence and not to disclose such information to any third " +
    "party without the prior written consent of the disclosing party."
);
body(
  "The obligations of confidentiality set forth in this Section shall survive the termination or " +
    "expiration of this Agreement for a period of three (3) years."
);

// Section 9 — Termination
sectionHeading("9", "Termination");
body(
  "Either party may terminate this Agreement upon sixty (60) days' written notice to the other party. " +
    "In the event of termination, Client shall pay Consultant for all services rendered and expenses " +
    "incurred through the effective date of termination. Any retainer amounts for services not yet " +
    "rendered shall be refunded to Client on a pro-rata basis within thirty (30) days of the " +
    "termination date."
);

// Section 10 — Governing Law
sectionHeading("10", "Governing Law and Dispute Resolution");
body(
  "This Agreement shall be governed by and construed in accordance with the laws of the State of " +
    "California, without regard to its conflict of laws principles. Any disputes arising out of or " +
    "relating to this Agreement shall be resolved through binding arbitration in San Francisco, " +
    "California, in accordance with the rules of the American Arbitration Association."
);

// Section 11 — Entire Agreement
sectionHeading("11", "Entire Agreement");
body(
  "This Agreement, together with any exhibits and amendments hereto, constitutes the entire agreement " +
    "between the parties with respect to the subject matter hereof and supersedes all prior " +
    "negotiations, representations, warranties, commitments, offers, contracts, and writings, " +
    "whether written or oral, with respect to the subject matter hereof."
);

// Signatures
doc.moveDown(1);
doc
  .fontSize(11)
  .font("Helvetica-Bold")
  .text("IN WITNESS WHEREOF, the parties have executed this Agreement as of the date first written above.");

signatureLine(
  "Maria Santos",
  "Managing Partner",
  "Meridian Consulting Group LLC",
  "June 15, 2024"
);

signatureLine(
  "James Richardson",
  "Chief Executive Officer",
  "Atlas Digital Solutions Inc",
  "June 15, 2024"
);

// Witnesses
doc.moveDown(1);
doc.fontSize(10).font("Helvetica-Bold").text("WITNESSES:");

signatureLine(
  "Elena Kowalski",
  "Senior Director, Technology Services",
  "Meridian Consulting Group LLC",
  "June 15, 2024"
);

signatureLine(
  "Roberto Fernandes",
  "VP of Engineering",
  "Atlas Digital Solutions Inc",
  "June 15, 2024"
);

// Finalize
doc.end();

stream.on("finish", () => {
  console.log(`PDF generated: ${outputPath}`);
});
