import { useTranslation } from "react-i18next";

const languages = [
  { code: "en", label: "English" },
  { code: "es", label: "Espa\u00f1ol" },
  { code: "pt", label: "Portugu\u00eas" },
];

export default function LanguageSwitcher() {
  const { i18n } = useTranslation();

  return (
    <select
      value={i18n.language?.substring(0, 2) || "en"}
      onChange={(e) => i18n.changeLanguage(e.target.value)}
      className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
    >
      {languages.map((lang) => (
        <option key={lang.code} value={lang.code}>
          {lang.label}
        </option>
      ))}
    </select>
  );
}
