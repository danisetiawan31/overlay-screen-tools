use std::fs;
use std::path::Path;

/// Representasi satu dokumen catatan Markdown dari Obsidian Vault.
#[derive(Debug, Clone, PartialEq)]
pub struct VaultDocument {
    pub file_name: String,
    pub relative_path: String,
    pub title: String,
    pub content: String,
}

/// Stopwords umum (Bahasa Indonesia & English) yang diabaikan saat pencarian kata kunci.
const STOPWORDS: &[&str] = &[
    // Indonesian stopwords
    "ada",
    "adalah",
    "adanya",
    "agar",
    "akan",
    "aku",
    "anda",
    "apa",
    "apakah",
    "atau",
    "bagaimana",
    "bahkan",
    "bahwa",
    "bang",
    "bisa",
    "boleh",
    "buat",
    "cuma",
    "dan",
    "dari",
    "dengan",
    "di",
    "dia",
    "dong",
    "gua",
    "gw",
    "halo",
    "harus",
    "hanya",
    "ia",
    "ini",
    "itu",
    "jika",
    "juga",
    "kalau",
    "kami",
    "kamu",
    "karena",
    "ke",
    "kenapa",
    "kita",
    "lagi",
    "lah",
    "lain",
    "lu",
    "mana",
    "mau",
    "mereka",
    "nah",
    "nih",
    "pada",
    "pernah",
    "saja",
    "sama",
    "saya",
    "sebagai",
    "sebuah",
    "sedang",
    "seperti",
    "sih",
    "sudah",
    "tapi",
    "tentang",
    "tersebut",
    "tidak",
    "untuk",
    "ya",
    "yang",
    // English stopwords
    "about",
    "after",
    "all",
    "also",
    "an",
    "and",
    "any",
    "are",
    "as",
    "at",
    "be",
    "because",
    "been",
    "before",
    "being",
    "between",
    "both",
    "but",
    "by",
    "can",
    "could",
    "did",
    "do",
    "does",
    "doing",
    "down",
    "during",
    "each",
    "few",
    "for",
    "from",
    "further",
    "had",
    "has",
    "have",
    "having",
    "he",
    "her",
    "here",
    "hers",
    "herself",
    "him",
    "himself",
    "his",
    "how",
    "if",
    "in",
    "into",
    "is",
    "it",
    "its",
    "itself",
    "just",
    "me",
    "more",
    "most",
    "my",
    "myself",
    "no",
    "nor",
    "not",
    "now",
    "of",
    "off",
    "on",
    "once",
    "only",
    "or",
    "other",
    "our",
    "ours",
    "ourselves",
    "out",
    "over",
    "own",
    "same",
    "she",
    "should",
    "so",
    "some",
    "such",
    "than",
    "that",
    "the",
    "their",
    "theirs",
    "them",
    "themselves",
    "then",
    "there",
    "these",
    "they",
    "this",
    "those",
    "through",
    "to",
    "too",
    "under",
    "until",
    "up",
    "very",
    "was",
    "we",
    "were",
    "what",
    "when",
    "where",
    "which",
    "while",
    "who",
    "whom",
    "why",
    "with",
    "would",
    "you",
    "your",
    "yours",
    "yourself",
    "yourselves",
];

/// Ekstraksi kata kunci yang bermakna dari teks query pertanyaan (minimal 2 karakter agar akronim tech seperti AI, DB, JS, UI, dsb tetap terjaring).
pub fn extract_keywords(query: &str) -> Vec<String> {
    let clean: String = query
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect();

    clean
        .split_whitespace()
        .filter(|w| w.len() >= 2 && !STOPWORDS.contains(w))
        .map(|w| w.to_string())
        .collect()
}

/// Ekstraksi judul catatan dari baris pertama heading `# Title` atau dari nama file.
pub fn extract_document_title(file_name: &str, content: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix('#') {
            let heading_title = heading.trim_start_matches('#').trim();
            if !heading_title.is_empty() {
                return heading_title.to_string();
            }
        }
    }

    file_name
        .strip_suffix(".md")
        .unwrap_or(file_name)
        .to_string()
}

/// Memindai folder Obsidian Vault secara rekursif dan memuat seluruh file `.md`.
/// Mengabaikan folder tersembunyi seperti `.obsidian`, `.git`, `.trash`, dan `node_modules`.
/// Membatasi pembacaan file individual maksimal 1 MB demi menjaga performa real-time.
pub fn scan_vault_markdown_files(vault_path: &Path) -> Result<Vec<VaultDocument>, String> {
    if !vault_path.exists() || !vault_path.is_dir() {
        return Err(format!(
            "Path vault Obsidian tidak valid atau bukan direktori: '{}'",
            vault_path.display()
        ));
    }

    let mut documents = Vec::new();
    let mut dirs_to_visit = vec![vault_path.to_path_buf()];

    while let Some(current_dir) = dirs_to_visit.pop() {
        let entries = match fs::read_dir(&current_dir) {
            Ok(read_dir) => read_dir,
            Err(err) => {
                eprintln!(
                    "[VAULT SCAN] Warning: Gagal membaca direktori '{}': {}",
                    current_dir.display(),
                    err
                );
                continue;
            }
        };

        for entry_res in entries {
            let entry = match entry_res {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            // Abaikan folder & file internal
            if file_name.starts_with('.')
                || file_name == "node_modules"
                || file_name == ".trash"
                || file_name == ".obsidian"
            {
                continue;
            }

            if path.is_dir() {
                dirs_to_visit.push(path);
            } else if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
                // Lewati file raksasa (> 1 MB) agar tidak membebani memori / disk I/O
                if let Ok(meta) = entry.metadata() {
                    if meta.len() > 1_000_000 {
                        continue;
                    }
                }

                match fs::read_to_string(&path) {
                    Ok(raw_content) => {
                        let content = raw_content.trim_start_matches('\u{feff}').to_string();
                        let relative_path = path
                            .strip_prefix(vault_path)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .replace('\\', "/");
                        let title = extract_document_title(&file_name, &content);

                        documents.push(VaultDocument {
                            file_name,
                            relative_path,
                            title,
                            content,
                        });
                    }
                    Err(err) => {
                        eprintln!(
                            "[VAULT SCAN] Warning: Gagal membaca file '{}': {}",
                            path.display(),
                            err
                        );
                    }
                }
            }
        }
    }

    Ok(documents)
}

const PROFILE_DOC_INDICATORS: &[&str] = &[
    "cv",
    "profile",
    "profil",
    "about",
    "biodata",
    "story-bank",
    "pengalaman",
    "experience",
    "intro",
    "bio",
    "star",
    "salary",
    "gaji",
    "snippets",
    "master",
];

const INTRO_QUERY_KEYWORDS: &[&str] = &[
    "perkenalkan",
    "diri",
    "dirimu",
    "intro",
    "introduce",
    "introduction",
    "profile",
    "profil",
    "cv",
    "background",
    "siapa",
    "pengalaman",
    "ceritakan",
    "salary",
    "gaji",
    "rate",
    "ekspektasi",
    "proyek",
    "project",
    "portofolio",
    "portfolio",
    "kelebihan",
    "kekurangan",
    "strength",
    "weakness",
    "alasan",
    "motivasi",
    "kenapa",
    "knp",
];

/// Menghitung skor relevansi satu dokumen terhadap kumpulan kata kunci query.
pub fn calculate_document_score(doc: &VaultDocument, keywords: &[String]) -> usize {
    if keywords.is_empty() {
        return 0;
    }

    let mut score = 0;
    let title_lower = doc.title.to_lowercase();
    let path_lower = doc.relative_path.to_lowercase();
    let content_lower = doc.content.to_lowercase();

    for kw in keywords {
        // Bobot tinggi: Kata kunci ada di judul atau path file (+10)
        if title_lower.contains(kw) || path_lower.contains(kw) {
            score += 10;
        }

        // Bobot isi: Frekuensi kemunculan kata kunci di dalam isi konten (+2 per kemunculan)
        let matches_count = content_lower.matches(kw).count();
        score += matches_count * 2;
    }

    // Bonus relevansi khusus jika pertanyaan seputar perkenalan diri dan dokumen adalah profile/cv/pengalaman
    let is_intro_query = keywords
        .iter()
        .any(|k| INTRO_QUERY_KEYWORDS.contains(&k.as_str()));
    let is_profile_doc = PROFILE_DOC_INDICATORS
        .iter()
        .any(|p| path_lower.contains(p) || title_lower.contains(p));

    if is_intro_query && is_profile_doc {
        score += 25;
    }

    score
}

/// Mengambil dan menyusun konteks teks catatan Obsidian yang paling relevan dengan query.
///
/// Mengembalikan `None` jika:
/// - Tidak ada kata kunci yang cocok (skor 0)
/// - Dokumen kosong
pub fn extract_relevant_context(
    documents: &[VaultDocument],
    query: &str,
    max_chars: usize,
) -> Option<String> {
    let keywords = extract_keywords(query);
    if keywords.is_empty() || documents.is_empty() {
        return None;
    }

    let mut scored_docs: Vec<(&VaultDocument, usize)> = documents
        .iter()
        .map(|doc| (doc, calculate_document_score(doc, &keywords)))
        .filter(|(_, score)| *score > 0)
        .collect();

    if scored_docs.is_empty() {
        // Fallback cerdas: jika tidak ada keyword spesifik, sertakan dokumen master profile utama
        let fallback_docs: Vec<&VaultDocument> = documents
            .iter()
            .filter(|d| {
                let p = d.relative_path.to_lowercase();
                p.contains("profile.md")
                    || p.contains("star_stories.md")
                    || p.contains("salary_matrix.md")
            })
            .take(2)
            .collect();

        if fallback_docs.is_empty() {
            return None;
        }

        let mut combined_context = String::new();
        for doc in fallback_docs {
            let max_doc_len = 1500;
            let truncated_content = if doc.content.len() > max_doc_len {
                let slice = &doc.content[..max_doc_len];
                format!("{}...\n[Konten catatan dipotong demi ringkasan]", slice)
            } else {
                doc.content.clone()
            };
            combined_context.push_str(&format!(
                "### [File: {}] (Master Profile Fallback)\n{}\n\n",
                doc.relative_path, truncated_content
            ));
        }

        return if combined_context.trim().is_empty() {
            None
        } else {
            Some(combined_context.trim().to_string())
        };
    }

    // Urutkan dari skor tertinggi ke terendah
    scored_docs.sort_by_key(|a| std::cmp::Reverse(a.1));

    let mut combined_context = String::new();
    let mut total_chars = 0;

    for (doc, score) in scored_docs.iter().take(3) {
        let max_doc_len = 1500; // Batasi panjang 1 dokumen agar dokumen lain tetap bisa masuk
        let truncated_content = if doc.content.len() > max_doc_len {
            let slice = &doc.content[..max_doc_len];
            format!("{}...\n[Konten catatan dipotong demi ringkasan]", slice)
        } else {
            doc.content.clone()
        };

        let section = format!(
            "### [File: {}] (Relevance Score: {})\n{}\n\n",
            doc.relative_path, score, truncated_content
        );

        if total_chars + section.len() > max_chars {
            // Jika melebihi batas total, ambil sebagian atau hentikan
            let remaining = max_chars.saturating_sub(total_chars);
            if remaining > 100 {
                combined_context.push_str(&section[..remaining]);
                combined_context.push_str("...\n");
            }
            break;
        }

        total_chars += section.len();
        combined_context.push_str(&section);
    }

    if combined_context.trim().is_empty() {
        None
    } else {
        Some(combined_context.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_keywords_removes_stopwords_and_short_words() {
        let query = "kamu pernah membuat project apa sih bang, apa urgensinya?";
        let keywords = extract_keywords(query);
        // "kamu", "pernah", "apa", "sih", "bang" adalah stopwords
        assert!(keywords.contains(&"membuat".to_string()));
        assert!(keywords.contains(&"project".to_string()));
        assert!(keywords.contains(&"urgensinya".to_string()));
        assert!(!keywords.contains(&"apa".to_string()));
        assert!(!keywords.contains(&"bang".to_string()));
    }

    #[test]
    fn test_extract_keywords_preserves_two_letter_tech_acronyms() {
        let query = "ceritakan pengalaman AI, UI/UX, dan database DB dengan JS/TS";
        let keywords = extract_keywords(query);
        assert!(keywords.contains(&"ai".to_string()));
        assert!(keywords.contains(&"ui".to_string()));
        assert!(keywords.contains(&"ux".to_string()));
        assert!(keywords.contains(&"db".to_string()));
        assert!(keywords.contains(&"js".to_string()));
        assert!(keywords.contains(&"ts".to_string()));
        assert!(!keywords.contains(&"dan".to_string())); // Stopword disaring
    }

    #[test]
    fn test_extract_document_title_from_heading_or_filename() {
        let content_with_heading = "# Arsitektur Microservices Project X\n\nDetail isi proyek...";
        let title1 = extract_document_title("project-x.md", content_with_heading);
        assert_eq!(title1, "Arsitektur Microservices Project X");

        let content_without_heading = "Hanya teks biasa tanpa heading...";
        let title2 = extract_document_title("my-notes.md", content_without_heading);
        assert_eq!(title2, "my-notes");
    }

    #[test]
    fn test_calculate_document_score_matches_title_and_content() {
        let doc1 = VaultDocument {
            file_name: "career-ops.md".to_string(),
            relative_path: "projects/career-ops.md".to_string(),
            title: "Career Ops Project".to_string(),
            content: "Urgensinya adalah mengotomasi proses lamaran kerja dan reporting."
                .to_string(),
        };

        let doc2 = VaultDocument {
            file_name: "recipe.md".to_string(),
            relative_path: "cooking/recipe.md".to_string(),
            title: "Resep Masakan".to_string(),
            content: "Membuat nasi goreng enak.".to_string(),
        };

        let keywords = vec!["project".to_string(), "urgensinya".to_string()];
        let score1 = calculate_document_score(&doc1, &keywords);
        let score2 = calculate_document_score(&doc2, &keywords);

        assert!(score1 > 0);
        assert_eq!(score2, 0);
    }

    #[test]
    fn test_extract_relevant_context_returns_none_when_no_match() {
        let docs = vec![VaultDocument {
            file_name: "test.md".to_string(),
            relative_path: "test.md".to_string(),
            title: "Test Note".to_string(),
            content: "Ini konten catatan biasa.".to_string(),
        }];

        let context = extract_relevant_context(&docs, "halo selamat pagi", 2000);
        assert_eq!(context, None);
    }

    #[test]
    fn test_extract_relevant_context_returns_formatted_content() {
        let docs = vec![
            VaultDocument {
                file_name: "project-alpha.md".to_string(),
                relative_path: "projects/project-alpha.md".to_string(),
                title: "Project Alpha".to_string(),
                content: "Project Alpha dibangun menggunakan Rust dan React.".to_string(),
            },
            VaultDocument {
                file_name: "unrelated.md".to_string(),
                relative_path: "notes/unrelated.md".to_string(),
                title: "Catatan Lain".to_string(),
                content: "Tidak ada hubungan.".to_string(),
            },
        ];

        let context = extract_relevant_context(&docs, "ceritakan tentang project alpha", 3000);
        assert!(context.is_some());
        let res = context.unwrap();
        assert!(res.contains("projects/project-alpha.md"));
        assert!(res.contains("Project Alpha dibangun menggunakan Rust dan React"));
        assert!(!res.contains("notes/unrelated.md"));
    }

    #[test]
    fn test_scan_vault_markdown_files_reads_directory_and_ignores_hidden() {
        let temp_dir = std::env::temp_dir().join("poc_overlay_test_vault_scanner");
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(temp_dir.join(".obsidian"));
        let _ = fs::create_dir_all(temp_dir.join("subfolder"));

        // Buat file yang harus terbaca
        let file1 = temp_dir.join("root.md");
        let _ = fs::write(&file1, "# Root Note\nKonten root");

        let file2 = temp_dir.join("subfolder").join("nested.md");
        let _ = fs::write(&file2, "# Nested Note\nKonten nested");

        // Buat file yang harus diabaikan
        let hidden_file = temp_dir.join(".obsidian").join("app.json");
        let _ = fs::write(&hidden_file, "{}");

        let non_md = temp_dir.join("image.png");
        let _ = fs::write(&non_md, [0u8; 10]);

        let docs = scan_vault_markdown_files(&temp_dir).unwrap();
        assert_eq!(docs.len(), 2);

        let titles: Vec<String> = docs.iter().map(|d| d.title.clone()).collect();
        assert!(titles.contains(&"Root Note".to_string()));
        assert!(titles.contains(&"Nested Note".to_string()));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_calculate_document_score_boosts_cv_and_profile_on_intro_queries() {
        let cv_doc = VaultDocument {
            file_name: "cv.md".to_string(),
            relative_path: "profile/cv.md".to_string(),
            title: "Curriculum Vitae".to_string(),
            content: "Pengalaman 5 tahun sebagai Fullstack Engineer di industri tech.".to_string(),
        };

        let cooking_doc = VaultDocument {
            file_name: "cooking.md".to_string(),
            relative_path: "notes/cooking.md".to_string(),
            title: "Cooking Notes".to_string(),
            content: "Resep makanan harian.".to_string(),
        };

        let intro_keywords = extract_keywords("Silahkan perkenalkan diri kamu");
        let cv_score = calculate_document_score(&cv_doc, &intro_keywords);
        let cooking_score = calculate_document_score(&cooking_doc, &intro_keywords);

        // CV doc harus mendapatkan boost skor tinggi (> 25)
        assert!(cv_score >= 25);
        assert_eq!(cooking_score, 0);
    }
}
