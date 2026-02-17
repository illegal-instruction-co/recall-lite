import { createContext, useContext, useState, useEffect, useCallback, useMemo } from "react";
import en from "./locales/en.json";
import tr from "./locales/tr.json";

type LocaleKey = keyof typeof en;
type LocaleMap = Record<LocaleKey, string>;

const locales: Record<string, LocaleMap> = { en, tr };

function getSystemLocale(): string {
    const lang = navigator.language?.split("-")[0] || "en";
    return lang in locales ? lang : "en";
}

interface LocaleContextType {
    locale: string;
    setLocale: (locale: string) => void;
    t: (key: LocaleKey, vars?: Record<string, string | number>) => string;
    availableLocales: string[];
}

const LocaleContext = createContext<LocaleContextType>({
    locale: "en",
    setLocale: () => { },
    t: (key) => key,
    availableLocales: Object.keys(locales),
});

export function LocaleProvider({ children }: Readonly<{ children: React.ReactNode }>) {
    const [locale, setLocaleState] = useState(() => {
        const saved = localStorage.getItem("recall-locale");
        return saved && saved in locales ? saved : getSystemLocale();
    });

    const setLocale = useCallback((newLocale: string) => {
        if (newLocale in locales) {
            setLocaleState(newLocale);
            localStorage.setItem("recall-locale", newLocale);
        }
    }, []);

    useEffect(() => {
        document.documentElement.lang = locale;
    }, [locale]);

    const t = useCallback(
        (key: LocaleKey, vars?: Record<string, string | number>): string => {
            let str = locales[locale]?.[key] || locales.en[key] || key;
            if (vars) {
                for (const [k, v] of Object.entries(vars)) {
                    str = str.replace(new RegExp(`\\{\\{${k}\\}\\}`, "g"), String(v));
                }
            }
            return str;
        },
        [locale]
    );

    const value = useMemo(
        () => ({ locale, setLocale, t, availableLocales: Object.keys(locales) }),
        [locale, setLocale, t]
    );

    return (
        <LocaleContext.Provider value={value}>
            {children}
        </LocaleContext.Provider>
    );
}

export function useLocale() {
    return useContext(LocaleContext);
}
