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
    const [currentLocale, setCurrentLocale] = useState(() => {
        const saved = localStorage.getItem("rememex-locale");
        return saved && saved in locales ? saved : getSystemLocale();
    });

    const setLocale = useCallback((newLocale: string) => {
        if (newLocale in locales) {
            setCurrentLocale(newLocale);
            localStorage.setItem("rememex-locale", newLocale);
        }
    }, []);

    useEffect(() => {
        document.documentElement.lang = currentLocale;
    }, [currentLocale]);

    const t = useCallback(
        (key: LocaleKey, vars?: Record<string, string | number>): string => {
            let str = locales[currentLocale]?.[key] || locales.en[key] || key;
            if (vars) {
                for (const [k, v] of Object.entries(vars)) {
                    str = str.replaceAll(`{{${k}}}`, String(v));
                }
            }
            return str;
        },
        [currentLocale]
    );

    const value = useMemo(
        () => ({ locale: currentLocale, setLocale, t, availableLocales: Object.keys(locales) }),
        [currentLocale, setLocale, t]
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
