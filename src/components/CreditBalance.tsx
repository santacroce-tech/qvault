import { useCredits } from "../hooks/useCredits";

export default function CreditBalance() {
  const { credits, loading } = useCredits();

  if (loading || !credits) return null;

  const freeText = `${credits.free_remaining} of 3 free documents remaining`;
  const paidText =
    credits.balance > 0 ? ` | ${credits.balance} paid credits` : "";

  return (
    <div className="mb-6 p-3 bg-blue-50 border border-blue-200 rounded-lg text-sm text-blue-700">
      {freeText}
      {paidText}
    </div>
  );
}
