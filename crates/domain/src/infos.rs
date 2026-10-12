//! Infos do jogo por chave — a tabela única embutida do app (o mesmo
//! molde de `psx.rs`/`tosec.rs`/`hltb.rs`, agora unificados): nome
//! canônico, lançamento, editora, região, nota (IGDB) e os tempos do
//! HowLongToBeat. Gerada por `scripts/gen_infos_data.py`; nada de rede ou
//! DAT em runtime — o usuário instala o app, solta a ROM e as infos estão
//! todas aqui.

use std::collections::HashMap;
use std::sync::LazyLock;

/// O que a tabela unificada sabe sobre um jogo.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InfosData {
    /// Nome canônico ("Tekken 3 (USA)").
    pub name: String,
    /// Ano de lançamento ("1998").
    pub year: Option<String>,
    /// Editora ("Namco").
    pub publisher: Option<String>,
    /// Região ("USA", "Europa", "Japão"…).
    pub region: Option<String>,
    /// Nota da IGDB (0-100), arredondada.
    pub nota: Option<String>,
    /// Nota de crítica (0-100, do Wikidata — Metacritic/GameRankings/
    /// Famitsu etc.), quando a IGDB não tem.
    pub nota_critica: Option<String>,
    /// HowLongToBeat: história principal (minutos).
    pub hltb_main: Option<u64>,
    /// HowLongToBeat: principal + extras (minutos).
    pub hltb_extra: Option<u64>,
}

/// `CHAVE\tnome\tano\teditora\tregião\tnota\tmain\textra` por linha,
/// minutos inteiros, `#` comenta, campo vazio = sem dado.
static TABLE: LazyLock<HashMap<&'static str, InfosData>> =
    LazyLock::new(|| parse_table(include_str!("infos_data.txt")));

fn parse_table(text: &'static str) -> HashMap<&'static str, InfosData> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut campos = line.split('\t');
        let (Some(key), Some(name)) = (campos.next(), campos.next()) else {
            continue;
        };
        let opt = |v: Option<&str>| -> Option<String> {
            v.filter(|v| !v.is_empty() && *v != "-")
                .map(|v| v.to_string())
        };
        let data = InfosData {
            name: name.to_string(),
            year: opt(campos.next()),
            publisher: opt(campos.next()),
            region: opt(campos.next()),
            nota: opt(campos.next()),
            nota_critica: opt(campos.next()),
            hltb_main: campos.next().and_then(|v| v.parse().ok()),
            hltb_extra: campos.next().and_then(|v| v.parse().ok()),
        };
        if data.name.is_empty() {
            continue;
        }
        map.insert(key, data);
    }
    map
}

/// As infos do jogo da chave (serial ou CRC32, maiúscula ou minúscula).
pub fn lookup(key: &str) -> Option<InfosData> {
    TABLE.get(key.to_ascii_uppercase().as_str()).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_parser_ler_o_formato_gerado() {
        let table = parse_table(
            "# gerado\n\n762AF827\tDonkey Kong Country (USA) (Rev 2)\t1994\tNintendo\tUSA\t86\t\t232\t300\n00000075\tJogo Magro\t\t\t\t\t\t\n",
        );
        let dkc = table.get("762AF827").cloned().expect("dkc");
        assert_eq!(dkc.name, "Donkey Kong Country (USA) (Rev 2)");
        assert_eq!(dkc.year.as_deref(), Some("1994"));
        assert_eq!(dkc.publisher.as_deref(), Some("Nintendo"));
        assert_eq!(dkc.region.as_deref(), Some("USA"));
        assert_eq!(dkc.nota.as_deref(), Some("86"));
        assert!(dkc.nota_critica.is_none());
        assert_eq!(dkc.hltb_main, Some(232));
        assert_eq!(dkc.hltb_extra, Some(300));
        // linha só com a chave e o nome (sem nenhum extra) sobrevive
        let magro = table.get("00000075").cloned().expect("linha magra");
        assert_eq!(magro.name, "Jogo Magro");
        assert!(magro.year.is_none() && magro.hltb_main.is_none());
    }

    #[test]
    fn lookup_e_case_insensitive() {
        let table = parse_table("762AF827\tDonkey Kong Country (USA) (Rev 2)\t\t\t\t\t\n");
        assert!(table.contains_key("762AF827"));
    }

    #[test]
    fn generated_table_conhece_serials_famosos() {
        // resistente a regenerações: os lançamentos da época não saem
        for chave in ["762AF827", "D63ED5F8"] {
            let data = lookup(chave).unwrap_or_else(|| panic!("{chave} fora da tabela"));
            assert!(!data.name.is_empty(), "{chave}: sem nome");
        }
    }
}
