#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    En,
    Tr,
}

impl Language {
    pub fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Tr => "tr",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Language::En => "English",
            Language::Tr => "Turkce",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Language::En => Language::Tr,
            Language::Tr => Language::En,
        }
    }

    pub fn from_code(code: &str) -> Self {
        match code {
            "tr" => Language::Tr,
            _ => Language::En,
        }
    }
}

pub fn detect_system_language() -> Language {
    if let Some(locale) = sys_locale::get_locale() {
        let lang = locale.split('-').next().unwrap_or("en");
        Language::from_code(lang)
    } else {
        Language::En
    }
}

fn get_string(lang: Language, key: &str) -> &'static str {
    match lang {
        Language::En => en(key),
        Language::Tr => {
            let val = tr(key);
            if val.is_empty() { en(key) } else { val }
        }
    }
}

pub fn t(lang: Language, key: &str, vars: &[(&str, &str)]) -> String {
    let mut s = get_string(lang, key).to_string();
    for (k, v) in vars {
        s = s.replace(&format!("{{{{{}}}}}", k), v);
    }
    s
}

pub fn ts(lang: Language, key: &str) -> String {
    get_string(lang, key).to_string()
}

fn en(key: &str) -> &'static str {
    match key {
        "search_placeholder" => "Search in {{container}}...",
        "index_folder_title" => "Index Folder into {{container}} (Ctrl+O)",
        "sidebar_title" => "Containers",
        "sidebar_collapse" => "Collapse sidebar",
        "sidebar_expand" => "Expand sidebar",
        "sidebar_create" => "Create Container",
        "sidebar_indexed_folders" => "Indexed Folders",
        "sidebar_no_folders" => "No folders indexed yet",
        "sidebar_rebuild" => "Rebuild Index",
        "sidebar_rebuild_tooltip" => "Re-index all folders with improved embeddings",
        "sidebar_clear" => "Clear Index",
        "sidebar_clear_tooltip" => "Remove all indexed data from this container",
        "sidebar_delete" => "Delete Container",
        "results_no_preview" => "No preview available",
        "results_no_results" => "No results found",
        "results_in_container" => "in {{container}}",
        "results_container_active" => "Container Active",
        "results_shortcuts" => "Shortcuts",
        "results_shortcut_index" => "Ctrl + O : Index",
        "results_shortcut_toggle" => "Alt + Space : Toggle",
        "results_navigate" => "to navigate",
        "results_open" => "to open",
        "status_indexed_folders" => "Indexed {{count}} folders",
        "modal_cancel" => "Cancel",
        "modal_ok" => "OK",
        "dialog_new_container" => "New Container",
        "dialog_field_name" => "Name",
        "dialog_field_name_placeholder" => "Work, Gaming, Research...",
        "dialog_field_description" => "Description (AI Context)",
        "dialog_field_description_placeholder" => "accounting files for acme corp",
        "dialog_create" => "Create",
        "dialog_delete_title" => "Delete Container",
        "dialog_delete_message" => "Are you sure you want to delete '{{name}}'? All indexed data will be lost forever.",
        "dialog_delete_confirm" => "Delete",
        "dialog_clear_title" => "Clear Index",
        "dialog_clear_message" => "Clear index for '{{name}}'?",
        "dialog_clear_confirm" => "Clear",
        "dialog_rebuild_title" => "Rebuild Index",
        "dialog_rebuild_message" => "This will re-index all {{count}} folder(s) in '{{name}}' with improved embeddings. This may take a moment.",
        "dialog_rebuild_confirm" => "Rebuild",
        "status_switched" => "Switched to {{name}}",
        "status_clearing" => "Clearing index...",
        "status_cleared" => "Index cleared.",
        "status_rebuilding" => "Rebuilding index...",
        "status_starting" => "Starting indexing...",
        "status_indexing_file" => "Indexing: {{filename}}",
        "status_result_count" => "{{count}} results",
        "status_done" => "Done -- {{message}}",
        "status_rebuild_needed" => "Index needs rebuild -- click Rebuild Index",
        "status_model_error" => "Model Error: {{error}}",
        "status_model_loading" => "Loading AI model...",
        _ => "???",
    }
}

fn tr(key: &str) -> &'static str {
    match key {
        "search_placeholder" => "{{container}} icinde ara...",
        "index_folder_title" => "{{container}} icin klasor indexle (Ctrl+O)",
        "sidebar_title" => "Konteynerler",
        "sidebar_collapse" => "Kenar cubugunu daralt",
        "sidebar_expand" => "Kenar cubugunu genislet",
        "sidebar_create" => "Konteyner Olustur",
        "sidebar_indexed_folders" => "Indexlenen Klasorler",
        "sidebar_no_folders" => "Henuz indexlenmis klasor yok",
        "sidebar_rebuild" => "Indexi Yeniden Olustur",
        "sidebar_rebuild_tooltip" => "Tum klasorleri gelistirilmis embeddinglerle yeniden indexle",
        "sidebar_clear" => "Indexi Temizle",
        "sidebar_clear_tooltip" => "Bu konteynerdeki tum indexlenmis verileri kaldir",
        "sidebar_delete" => "Konteyneri Sil",
        "results_no_preview" => "Onizleme yok",
        "results_no_results" => "Sonuc bulunamadi",
        "results_in_container" => "{{container}} icinde",
        "results_container_active" => "Konteyner Aktif",
        "results_shortcuts" => "Kisayollar",
        "results_shortcut_index" => "Ctrl + O : Indexle",
        "results_shortcut_toggle" => "Alt + Space : Ac/Kapat",
        "results_navigate" => "gezinmek icin",
        "results_open" => "acmak icin",
        "status_indexed_folders" => "{{count}} klasor indexlendi",
        "modal_cancel" => "Iptal",
        "modal_ok" => "Tamam",
        "dialog_new_container" => "Yeni Konteyner",
        "dialog_field_name" => "Isim",
        "dialog_field_name_placeholder" => "Is, Oyun, Arastirma...",
        "dialog_field_description" => "Aciklama (AI Baglami)",
        "dialog_field_description_placeholder" => "acme sirketi icin muhasebe dosyalari",
        "dialog_create" => "Olustur",
        "dialog_delete_title" => "Konteyneri Sil",
        "dialog_delete_message" => "'{{name}}' silinsin mi? Tum indexlenmis veriler kalici olarak kaybolacak.",
        "dialog_delete_confirm" => "Sil",
        "dialog_clear_title" => "Indexi Temizle",
        "dialog_clear_message" => "'{{name}}' icin index temizlensin mi?",
        "dialog_clear_confirm" => "Temizle",
        "dialog_rebuild_title" => "Indexi Yeniden Olustur",
        "dialog_rebuild_message" => "Bu islem '{{name}}' icindeki {{count}} klasoru gelistirilmis embeddinglerle yeniden indexleyecek. Biraz zaman alabilir.",
        "dialog_rebuild_confirm" => "Yeniden Olustur",
        "status_switched" => "{{name}} konteynerine gecildi",
        "status_clearing" => "Index temizleniyor...",
        "status_cleared" => "Index temizlendi.",
        "status_rebuilding" => "Index yeniden olusturuluyor...",
        "status_starting" => "Indexleme basliyor...",
        "status_indexing_file" => "Indexleniyor: {{filename}}",
        "status_result_count" => "{{count}} sonuc",
        "status_done" => "Tamamlandi -- {{message}}",
        "status_rebuild_needed" => "Index yeniden olusturulmali -- Yeniden Olustur'a tiklayin",
        "status_model_error" => "Model Hatasi: {{error}}",
        "status_model_loading" => "AI modeli yukleniyor...",
        _ => "",
    }
}
