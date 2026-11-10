---
name: egui-qt-fusion
description: Gestalte und überarbeite Rust-egui-Oberflächen im Stil von Qt Fusion Dark oder Light. Verwenden, wenn ein egui/eframe-Projekt ein konsistentes Qt-Fusion-artiges Theme, passende Widgets oder einen Light/Dark-Umschalter braucht.
---

# egui im Qt-Fusion-Stil gestalten

## Ziel

Gestalte egui-Oberflächen mit der nüchternen, kompakten und plattformneutralen Anmutung von **Qt Fusion**. Unterstütze sowohl **Dark** als auch **Light**. Fusion ist eine visuelle Referenz, keine pixelgenaue Qt-Implementierung.

Bevorzuge:

- klare Flächen und feine Rahmen statt Schatten und Verläufen
- kompakte, konsistente Abstände
- gut lesbare Typografie und eindeutige Zustände
- zurückhaltende Akzentfarben
- eguis native Widgets und Interaktionen

Vermeide:

- Web-Dashboard-Ästhetik, übergroße Karten und pillenförmige Controls
- starke Schatten, Glas-/Neumorphismus-Effekte und unnötige Animationen
- eigene Widget-Nachbauten, wenn ein egui-Widget genügt

## Vorgehen

1. Prüfe zuerst `Cargo.toml`, egui-/eframe-Version und vorhandene Theme- oder Style-Logik.
2. Verwende die API der tatsächlich vorhandenen egui-Version. Erfinde keine Methoden oder Felder; prüfe bei Unsicherheit die lokalen Quellen oder Dokumentation.
3. Zentralisiere die Gestaltung in einer Funktion wie `fusion_visuals(dark: bool) -> egui::Visuals` und gegebenenfalls `fusion_style() -> egui::Style`.
4. Setze das Theme an einer zentralen Stelle, typischerweise über `Context::set_visuals` und `Context::set_style`. Verteile keine unkoordinierten Style-Änderungen über einzelne Views.
5. Erhalte bestehendes Layout und Verhalten, sofern der Auftrag nicht ausdrücklich eine Neugestaltung verlangt.
6. Kontrolliere Hover-, Aktiv-, Fokus-, Selektions- und Deaktiviert-Zustände sowie Lesbarkeit in beiden Themes.

## Design-Tokens

Nutze diese Werte als Startpunkt, passe sie aber an Kontrast und die vorhandene App an. RGB-Werte sind sRGB.

| Token | Dark | Light |
|---|---:|---:|
| Fensterhintergrund | `#353535` | `#EFEFEF` |
| Panel / Fläche | `#353535` | `#F4F4F4` |
| Eingabefeld | `#232323` | `#FFFFFF` |
| Primärtext | `#F0F0F0` | `#202020` |
| Sekundärtext | `#B8B8B8` | `#606060` |
| Rahmen | `#555555` | `#A0A0A0` |
| Hover | `#454545` | `#E2E2E2` |
| Gedrückt / aktiv | `#2A82DA` | `#308CC6` |
| Auswahl | `#2A82DA` | `#308CC6` |
| Fehlermeldung | `#F06A6A` | `#C62828` |
| Warnung | `#E6B450` | `#8A5A00` |

Akzentfarben müssen auf der jeweiligen Fläche lesbar sein. Für Text auf einer kräftigen Akzentfläche einen passenden kontrastreichen Textfarbton festlegen; nicht ungeprüft die normale Textfarbe übernehmen.

## egui-Umsetzung

- Erzeuge Farben mit `egui::Color32::from_rgb(r, g, b)` oder einer gleichwertigen Hilfsfunktion.
- Lege `Visuals` ausgehend von `Visuals::dark()` beziehungsweise `Visuals::light()` an und überschreibe gezielt die benötigten Felder. So bleiben versionsabhängige Defaults möglichst erhalten.
- Passe insbesondere Fenster-/Panel-Flächen, Widget-Zustände (`inactive`, `hovered`, `active`, `open`), Textfarben, Auswahl und Rahmen konsistent an.
- Verwende geringe Eckenradien und dünne Rahmen. Einheitliche Werte sind wichtiger als eine exakte Qt-Messung.
- Setze Innenabstände und Abstände über Layout-/Style-Spacings konsistent und eher kompakt. Vermeide negative Spacings als Mittel für gewöhnliches Layouting.
- `Visuals` allein stylt nicht jedes Detail. Für individuelle Widgets lokale `Frame`, `Button` oder `Stroke`-Anpassungen nur dort einsetzen, wo sie nötig sind.
- Richte die exakten Feldnamen und Typen nach der im Projekt verwendeten egui-Version. Ältere und neuere egui-Versionen können sich unterscheiden.

## Beispielgerüst

Das folgende Gerüst ist absichtlich auf die vorhandene egui-Version anzupassen; vor dem Einfügen die API-Felder prüfen:

```rust
fn fusion_visuals(dark: bool) -> egui::Visuals {
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    let rgb = egui::Color32::from_rgb;

    if dark {
        visuals.panel_fill = rgb(53, 53, 53);
        visuals.window_fill = rgb(53, 53, 53);
        visuals.override_text_color = Some(rgb(240, 240, 240));
        visuals.selection.bg_fill = rgb(42, 130, 218);
    } else {
        visuals.panel_fill = rgb(244, 244, 244);
        visuals.window_fill = rgb(239, 239, 239);
        visuals.override_text_color = Some(rgb(32, 32, 32));
        visuals.selection.bg_fill = rgb(48, 140, 198);
    }

    // Weitere Widget-Zustände und Rahmen für die Projektversion ergänzen.
    visuals
}
```
