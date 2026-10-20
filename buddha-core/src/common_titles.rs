//! Common names of major scriptures → canonical text IDs.
//!
//! Title indexes only see full canonical titles (妙法蓮華經, 大毘盧遮那成佛神變加持經),
//! so everyday names such as 法華経 or 大日経 rank commentaries first. Every local ID
//! below was checked against the `<title level="m">` of the bundled CBETA XML
//! (2026-10-20); `local: false` entries (Taishō vols. 56–84) are read through SAT.

use crate::text_utils::fold_cjk_variants;

pub struct CommonTitle {
    pub names: &'static [&'static str],
    pub id: &'static str,
    pub title: &'static str,
    /// Present in local CBETA (`cbeta-fetch`); otherwise use SAT / SHINZEN.
    pub local: bool,
}

const fn t(names: &'static [&'static str], id: &'static str, title: &'static str) -> CommonTitle {
    CommonTitle {
        names,
        id,
        title,
        local: true,
    }
}

const fn sat(names: &'static [&'static str], id: &'static str, title: &'static str) -> CommonTitle {
    CommonTitle {
        names,
        id,
        title,
        local: false,
    }
}

pub const COMMON_TITLES: &[CommonTitle] = &[
    t(&["法華経", "妙法蓮華経"], "T09n0262", "妙法蓮華經"),
    t(&["華厳経", "八十華厳"], "T10n0279", "大方廣佛華嚴經"),
    t(&["六十華厳"], "T09n0278", "大方廣佛華嚴經"),
    t(&["般若心経", "心経"], "T08n0251", "般若波羅蜜多心經"),
    t(&["金剛経", "金剛般若経"], "T08n0235", "金剛般若波羅蜜經"),
    t(&["大般若経"], "T05n0220a", "大般若波羅蜜多經"),
    t(&["大日経"], "T18n0848", "大毘盧遮那成佛神變加持經"),
    t(&["大日経疏"], "T39n1796", "大毘盧遮那成佛經疏"),
    t(
        &["金剛頂経"],
        "T18n0865",
        "金剛頂一切如來真實攝大乘現證大教王經",
    ),
    t(&["理趣経"], "T08n0243", "大樂金剛不空真實三麼耶經"),
    t(
        &["理趣釈"],
        "T19n1003",
        "大樂金剛不空真實三昧耶經般若波羅蜜多理趣釋",
    ),
    t(&["蘇悉地経"], "T18n0893a", "蘇悉地羯羅經"),
    t(&["瞿醯経"], "T18n0897", "蕤呬耶經"),
    t(
        &["守護国界主陀羅尼経", "守護経"],
        "T19n0997",
        "守護國界主陀羅尼經",
    ),
    t(&["無量寿経"], "T12n0360", "佛說無量壽經"),
    t(&["観無量寿経"], "T12n0365", "佛說觀無量壽佛經"),
    t(&["阿弥陀経"], "T12n0366", "佛說阿彌陀經"),
    t(&["涅槃経", "大般涅槃経"], "T12n0374", "大般涅槃經"),
    t(&["南本涅槃経"], "T12n0375", "大般涅槃經"),
    t(&["維摩経"], "T14n0475", "維摩詰所說經"),
    t(&["勝鬘経"], "T12n0353", "勝鬘師子吼一乘大方便方廣經"),
    t(&["楞伽経"], "T16n0670", "楞伽阿跋多羅寶經"),
    t(
        &["薬師経", "薬師如来本願経"],
        "T14n0450",
        "藥師琉璃光如來本願功德經",
    ),
    t(&["地蔵菩薩本願経", "地蔵経"], "T13n0412", "地藏菩薩本願經"),
    t(&["金光明経"], "T16n0663", "金光明經"),
    t(
        &["金光明最勝王経", "最勝王経"],
        "T16n0665",
        "金光明最勝王經",
    ),
    t(&["仁王経"], "T08n0245", "佛說仁王般若波羅蜜經"),
    t(&["仁王護国般若経"], "T08n0246", "仁王護國般若波羅蜜多經"),
    t(
        &["首楞厳経"],
        "T19n0945",
        "大佛頂如來密因修證了義諸菩薩萬行首楞嚴經",
    ),
    t(&["円覚経"], "T17n0842", "大方廣圓覺修多羅了義經"),
    t(&["梵網経"], "T24n1484", "梵網經"),
    t(&["遺教経"], "T12n0389", "佛垂般涅槃略說教誡經"),
    t(&["四十二章経"], "T17n0784", "四十二章經"),
    t(&["盂蘭盆経"], "T16n0685", "佛說盂蘭盆經"),
    t(
        &["千手経", "千手陀羅尼経"],
        "T20n1060",
        "千手千眼觀世音菩薩廣大圓滿無礙大悲心陀羅尼經",
    ),
    t(
        &["十一面観音神呪経", "十一面経"],
        "T20n1070",
        "佛說十一面觀世音神呪經",
    ),
    t(&["不空羂索経"], "T20n1092", "不空羂索神變真言經"),
    t(
        &["尊勝陀羅尼経", "仏頂尊勝陀羅尼経"],
        "T19n0967",
        "佛頂尊勝陀羅尼經",
    ),
    t(
        &["宝篋印陀羅尼経"],
        "T19n1022A",
        "一切如來心祕密全身舍利寶篋印陀羅尼經",
    ),
    t(&["法句経"], "T04n0210", "法句經"),
    t(&["長阿含経"], "T01n0001", "長阿含經"),
    t(&["中阿含経"], "T01n0026", "中阿含經"),
    t(&["雑阿含経"], "T02n0099", "雜阿含經"),
    t(&["増一阿含経"], "T02n0125", "增壹阿含經"),
    t(&["大智度論"], "T25n1509", "大智度論"),
    t(&["倶舎論"], "T29n1558", "阿毘達磨俱舍論"),
    t(&["成唯識論"], "T31n1585", "成唯識論"),
    t(&["中論"], "T30n1564", "中論"),
    t(&["瑜伽師地論"], "T30n1579", "瑜伽師地論"),
    t(&["大乗起信論", "起信論"], "T32n1666", "大乘起信論"),
    t(&["釈摩訶衍論"], "T32n1668", "釋摩訶衍論"),
    t(
        &["菩提心論"],
        "T32n1665",
        "金剛頂瑜伽中發阿耨多羅三藐三菩提心論",
    ),
    t(&["摩訶止観"], "T46n1911", "摩訶止觀"),
    t(&["六祖壇経", "壇経"], "T48n2008", "六祖大師法寶壇經"),
    sat(
        &["十住心論", "秘密曼荼羅十住心論"],
        "T77n2425",
        "祕密曼荼羅十住心論",
    ),
    sat(&["秘蔵宝鑰"], "T77n2426", "祕藏寶鑰"),
    sat(&["弁顕密二教論", "二教論"], "T77n2427", "辯顯密二教論"),
    sat(&["即身成仏義"], "T77n2428", "即身成佛義"),
    sat(&["声字実相義"], "T77n2429", "聲字實相義"),
    sat(&["吽字義"], "T77n2430", "吽字義"),
    sat(&["般若心経秘鍵", "心経秘鍵"], "T57n2203", "般若心經祕鍵"),
];

/// Entries whose common name equals the query (CJK variants, spacing and
/// punctuation ignored).
pub fn common_title_matches(q: &str) -> Vec<&'static CommonTitle> {
    let fq = fold_cjk_variants(q);
    if fq.is_empty() {
        return Vec::new();
    }
    COMMON_TITLES
        .iter()
        .filter(|c| {
            c.names
                .iter()
                .chain(std::iter::once(&c.title))
                .any(|n| fold_cjk_variants(n) == fq)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_names_resolve_across_variants() {
        assert_eq!(common_title_matches("法華経")[0].id, "T09n0262");
        assert_eq!(common_title_matches("法華經")[0].id, "T09n0262");
        assert_eq!(common_title_matches(" 大日経 ")[0].id, "T18n0848");
        assert_eq!(common_title_matches("即身成佛義")[0].id, "T77n2428");
        assert!(!common_title_matches("即身成佛義")[0].local);
        assert!(common_title_matches("大経").is_empty());
        assert!(common_title_matches("").is_empty());
    }
}
