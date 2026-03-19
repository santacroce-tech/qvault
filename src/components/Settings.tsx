import { useState } from "react";
import { useTranslation } from "react-i18next";
import CreditBalance from "./CreditBalance";
import LicenseActivation from "./LicenseActivation";
import LanguageSwitcher from "./LanguageSwitcher";

export default function Settings() {
  const { t } = useTranslation();
  const [jurisdiction, setJurisdiction] = useState("global");
  const [sensitivity, setSensitivity] = useState(50);

  return (
    <div className="flex-1 p-6 bg-content">
      <div className="max-w-lg mx-auto">
        <h2 className="text-xl font-bold text-gray-900 mb-6">{t('settings.title')}</h2>

        {/* Credits & License */}
        <section className="mb-8">
          <h3 className="text-sm font-semibold text-gray-700 mb-3 uppercase tracking-wider">
            {t('settings.licenseCredits')}
          </h3>
          <CreditBalance />
          <LicenseActivation />
        </section>

        {/* Language */}
        <section className="mb-8">
          <h3 className="text-sm font-semibold text-gray-700 mb-3 uppercase tracking-wider">
            {t('settings.language')}
          </h3>
          <LanguageSwitcher />
        </section>

        {/* Detection Settings */}
        <section className="mb-8">
          <h3 className="text-sm font-semibold text-gray-700 mb-3 uppercase tracking-wider">
            {t('settings.detection')}
          </h3>

          <div className="bg-white rounded-lg border border-gray-200 p-4 space-y-4">
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                {t('settings.jurisdiction')}
              </label>
              <select
                value={jurisdiction}
                onChange={(e) => setJurisdiction(e.target.value)}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
              >
                <option value="global">{t('settings.jurisdictionGlobal')}</option>
                <option value="us">{t('settings.jurisdictionUS')}</option>
                <option value="eu">{t('settings.jurisdictionEU')}</option>
                <option value="br">{t('settings.jurisdictionBR')}</option>
                <option value="de">{t('settings.jurisdictionDE')}</option>
              </select>
            </div>

            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                {t('settings.sensitivity', { value: sensitivity })}
              </label>
              <input
                type="range"
                min={0}
                max={100}
                value={sensitivity}
                onChange={(e) => setSensitivity(Number(e.target.value))}
                className="w-full"
              />
              <div className="flex justify-between text-xs text-gray-400 mt-1">
                <span>{t('settings.fewerDetections')}</span>
                <span>{t('settings.moreDetections')}</span>
              </div>
            </div>
          </div>
        </section>

        {/* Data Retention */}
        <section>
          <h3 className="text-sm font-semibold text-gray-700 mb-3 uppercase tracking-wider">
            {t('settings.dataRetention')}
          </h3>
          <div className="bg-white rounded-lg border border-gray-200 p-4">
            <p className="text-sm text-gray-600 mb-3">
              {t('settings.dataRetentionDesc')}
            </p>
            <button className="px-4 py-2 bg-red-50 text-red-600 text-sm rounded-lg hover:bg-red-100 transition-colors">
              {t('settings.clearAll')}
            </button>
          </div>
        </section>
      </div>
    </div>
  );
}
