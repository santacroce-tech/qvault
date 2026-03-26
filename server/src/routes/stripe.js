import { Router } from "express";
import express from "express";
import Stripe from "stripe";
import { insertLicenseKey, insertPurchase, findPurchaseBySessionId } from "../db.js";
import { generateLicenseKey } from "../utils/keygen.js";

const router = Router();

function getStripe() {
  if (!process.env.STRIPE_SECRET_KEY) {
    throw new Error("STRIPE_SECRET_KEY not configured");
  }
  return new Stripe(process.env.STRIPE_SECRET_KEY);
}

/**
 * POST /api/stripe/create-checkout-session
 *
 * Creates a Stripe Checkout session for the yearly plan.
 * Body: { locale?: string }
 * Returns: { url: string }
 */
router.post("/create-checkout-session", async (req, res) => {
  try {
    const stripe = getStripe();
    const priceId = process.env.STRIPE_PRICE_ID;

    if (!priceId) {
      return res.status(500).json({ error: "Stripe price not configured" });
    }

    const locale = req.body?.locale || "auto";

    const session = await stripe.checkout.sessions.create({
      mode: "payment",
      line_items: [{ price: priceId, quantity: 1 }],
      success_url: "https://qvault.tech/success.html?session_id={CHECKOUT_SESSION_ID}",
      cancel_url: "https://qvault.tech/#pricing",
      locale: ["en", "es", "pt"].includes(locale) ? locale : "auto",
      payment_method_types: ["card"],
      metadata: {
        product: "qvault-yearly",
      },
    });

    return res.json({ url: session.url });
  } catch (err) {
    console.error("Stripe checkout error:", err);
    return res.status(500).json({ error: "Failed to create checkout session" });
  }
});

/**
 * POST /api/stripe/webhook
 *
 * Stripe webhook endpoint. Must receive raw body for signature verification.
 */
router.post("/webhook", express.raw({ type: "application/json" }), async (req, res) => {
  const stripe = getStripe();
  const sig = req.headers["stripe-signature"];
  const webhookSecret = process.env.STRIPE_WEBHOOK_SECRET;

  if (!webhookSecret) {
    console.error("STRIPE_WEBHOOK_SECRET not configured");
    return res.status(500).send("Webhook not configured");
  }

  let event;
  try {
    event = stripe.webhooks.constructEvent(req.body, sig, webhookSecret);
  } catch (err) {
    console.error("Webhook signature verification failed:", err.message);
    return res.status(400).send(`Webhook Error: ${err.message}`);
  }

  if (event.type === "checkout.session.completed") {
    const session = event.data.object;

    // Idempotency check
    const existing = findPurchaseBySessionId(session.id);
    if (existing) {
      console.log("Webhook: duplicate session", session.id, "— already processed");
      return res.json({ received: true });
    }

    try {
      // Generate license key
      const licenseKey = generateLicenseKey();
      const result = insertLicenseKey(licenseKey, "yearly", 2);

      // Record purchase
      const email = session.customer_details?.email || session.customer_email || null;
      const amountCents = session.amount_total || 29900;
      const currency = session.currency || "usd";
      insertPurchase(session.id, email, result.lastInsertRowid, amountCents, currency);

      console.log(`Webhook: checkout complete — key=${licenseKey}, email=${email}, amount=${amountCents}`);
    } catch (err) {
      console.error("Webhook: failed to process checkout:", err);
      return res.status(500).json({ error: "Processing failed" });
    }
  }

  return res.json({ received: true });
});

/**
 * GET /api/stripe/session-info?session_id=xxx
 *
 * Returns the license key for a completed checkout session.
 * Used by the success page to display the key.
 */
router.get("/session-info", async (req, res) => {
  const sessionId = req.query.session_id;
  if (!sessionId) {
    return res.status(400).json({ error: "Missing session_id" });
  }

  // Check our DB first
  const purchase = findPurchaseBySessionId(sessionId);
  if (purchase) {
    return res.json({
      email: purchase.customer_email,
      license_key: purchase.license_key,
    });
  }

  // If not in DB yet (webhook may be delayed), return pending
  return res.status(404).json({ error: "Session not found. Payment may still be processing." });
});

export default router;
