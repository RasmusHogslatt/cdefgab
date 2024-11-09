use roxmltree::Node;

use super::TimeSignature;

pub fn extract_score_metadata(root: &Node) -> (u8, TimeSignature, usize) {
    let divisions_per_quarter = root
        .descendants()
        .find(|n| n.has_tag_name("divisions"))
        .and_then(|n| n.text().map(|t| t.parse::<u8>().unwrap_or(1)))
        .unwrap_or(1);

    let beats_per_measure = root
        .descendants()
        .find(|n| n.has_tag_name("time"))
        .and_then(|n| {
            n.descendants()
                .find(|m| m.has_tag_name("beats"))
                .and_then(|b| b.text().map(|t| t.parse::<u8>().unwrap_or(0)))
        })
        .unwrap_or(4);

    let beat_value = root
        .descendants()
        .find(|n| n.has_tag_name("time"))
        .and_then(|n| {
            n.descendants()
                .find(|m| m.has_tag_name("beat-type"))
                .and_then(|b| b.text().map(|t| t.parse::<u8>().unwrap_or(0)))
        })
        .unwrap_or(4);

    let tempo = root
        .descendants()
        .find(|n| n.has_tag_name("sound") && n.attribute("tempo").is_some())
        .and_then(|n| {
            n.attribute("tempo")
                .map(|t| t.parse::<usize>().unwrap_or(120))
        })
        .unwrap_or(120);

    let time_signature = TimeSignature {
        beats_per_measure,
        beat_value,
    };

    (divisions_per_quarter, time_signature, tempo)
}

pub fn calculate_divisions_per_measure(
    beats_per_measure: u8,
    divisions_per_quarter: u8,
    beat_value: u8,
) -> usize {
    (beats_per_measure as usize) * (divisions_per_quarter as usize) * 4 / (beat_value as usize)
}
