//! Thallium-styled window: Deep Space palette, chamfered plates (top-right
//! cut, full-perimeter stroke only), Unbounded / Albert Sans / JetBrains Mono.

slint::slint! {
    import { ScrollView } from "std-widgets.slint";

    export struct Req { label: string, value: string, note: string, level: int }
    export struct Drive { name: string, detail: string }

    global T {
        out property <color> base: #0d0d0f;
        out property <color> panel: #13161a;
        out property <color> plate: #d3c6aa08;
        out property <color> line-dead: #232a2e;
        out property <color> line: #3a464c;
        out property <color> fg: #d3c6aa;
        out property <color> muted: #859289;
        out property <color> green: #a7c080;
        out property <color> yellow: #dbbc7f;
        out property <color> red: #e67e80;
        out property <length> cut: 14px;
        out property <string> brand: "Unbounded";
        out property <string> human: "Albert Sans";
        out property <string> mono: "JetBrains Mono";
        public pure function level-color(l: int) -> color {
            return l == 0 ? green : l == 1 ? yellow : l == 2 ? red : muted;
        }
    }

    // The chamfer outline alone, filling its parent. Used for decoration.
    component Chamfer inherits Path {
        in property <length> cut: T.cut;
        width: 100%; height: 100%;
        viewbox-width: self.width / 1px;
        viewbox-height: self.height / 1px;
        fill: T.plate;
        stroke: T.line-dead;
        stroke-width: 1px;
        commands: "M 0.5 0.5 L " + (self.width / 1px - self.cut / 1px) + " 0.5 L "
            + (self.width / 1px - 0.5) + " " + (self.cut / 1px) + " L "
            + (self.width / 1px - 0.5) + " " + (self.height / 1px - 0.5) + " L 0.5 "
            + (self.height / 1px - 0.5) + " Z";
    }

    // Chamfered plate: the corner is geometry, and the stroke runs the whole perimeter.
    component Plate inherits Rectangle {
        vertical-stretch: 0;
        in property <color> stroke: T.line-dead;
        in property <color> fill: T.plate;
        in property <length> cut: T.cut;
        in property <length> stroke-w: 1px;
        in property <length> pad: 18px;
        in property <length> gap: 10px;
        Chamfer { cut: root.cut; fill: root.fill; stroke: root.stroke; stroke-width: root.stroke-w; }
        VerticalLayout {
            padding: root.pad;
            spacing: root.gap;
            alignment: start;
            @children
        }
    }

    component Label inherits Text {
        font-family: T.mono;
        font-size: 11px;
        letter-spacing: 1.2px;
        color: T.muted;
    }

    component Body inherits Text {
        font-family: T.human;
        font-size: 14px;
        color: T.fg;
        wrap: word-wrap;
    }

    component Btn inherits Rectangle {
        in property <string> text;
        in property <bool> primary;
        in property <bool> enabled: true;
        callback clicked();
        min-height: 40px;
        min-width: 130px;
        horizontal-stretch: 0;
        Chamfer {
            
            cut: 10px; stroke-width: 1.4px;
            stroke: !root.enabled ? T.line-dead : (touch.has-hover || root.primary) ? T.green : T.line;
            fill: root.primary && root.enabled ? (touch.pressed ? #a7c08040 : #a7c08022) : T.plate;
        }
        Text {
            text: root.text;
            font-family: T.mono; font-size: 12px; font-weight: 700; letter-spacing: 1.4px;
            color: root.enabled ? (root.primary ? T.green : T.fg) : T.muted;
        }
        touch := TouchArea {
            enabled: root.enabled;
            mouse-cursor: pointer;
            clicked => { root.clicked(); }
        }
    }

    component Toggle inherits HorizontalLayout {
        in-out property <bool> checked;
        in property <string> text;
        in property <bool> enabled: true;
        spacing: 12px;
        Rectangle {
            width: 20px; height: 20px;
            y: (parent.height - self.height) / 2;
            Chamfer {
                 cut: 6px;
                stroke: root.checked ? T.green : T.line;
                fill: root.checked ? #a7c08033 : T.plate;
            }
            if root.checked: Rectangle { width: 8px; height: 8px; background: T.green; }
        }
        Body { text: root.text; vertical-alignment: center; color: root.enabled ? T.fg : T.muted; }
        TouchArea {
            enabled: root.enabled;
            mouse-cursor: pointer;
            clicked => { root.checked = !root.checked; }
        }
    }

    component Chip inherits Rectangle {
        in property <string> text;
        in property <bool> selected;
        callback clicked();
        height: 34px;
        Chamfer {
             cut: 8px;
            stroke: root.selected ? T.green : touch.has-hover ? T.line : T.line-dead;
            fill: root.selected ? #a7c0801a : T.plate;
        }
        Text { text: root.text; font-family: T.mono; font-size: 12px; color: root.selected ? T.green : T.fg; }
        touch := TouchArea { mouse-cursor: pointer; clicked => { root.clicked(); } }
    }

    component Input inherits HorizontalLayout {
        in property <string> label;
        in property <string> placeholder;
        in property <bool> secret;
        in-out property <string> text;
        spacing: 14px;
        Label { text: root.label; width: 130px; vertical-alignment: center; }
        Rectangle {
            height: 36px;
            Chamfer {
                 cut: 8px;
                stroke: input.has-focus ? T.green : T.line-dead;
            }
            input := TextInput {
                x: 12px; width: parent.width - 24px;
                single-line: true;
                text <=> root.text;
                input-type: root.secret ? InputType.password : InputType.text;
                font-family: T.human; font-size: 14px; color: T.fg;
                vertical-alignment: center;
            }
            if root.text == "": Text {
                x: 12px; text: root.placeholder; font-family: T.human; font-size: 14px; color: #85928980;
            }
        }
    }

    component ReqRow inherits Plate {
        in property <Req> req;
        pad: 14px;
        HorizontalLayout {
            spacing: 14px;
            Rectangle {
                width: 52px; height: 24px;
                y: (parent.height - self.height) / 2;
                Chamfer {  cut: 6px; stroke: T.level-color(req.level); fill: T.plate; }
                Text {
                    text: req.level == 0 ? "OK" : req.level == 1 ? "NOTE" : req.level == 2 ? "FAIL" : "INFO";
                    font-family: T.mono; font-size: 10px; font-weight: 700; color: T.level-color(req.level);
                }
            }
            VerticalLayout {
                spacing: 3px;
                HorizontalLayout {
                    spacing: 10px;
                    Label { text: req.label; vertical-alignment: center; }
                    Text { text: req.value; font-family: T.human; font-size: 14px; font-weight: 600; color: T.fg; horizontal-stretch: 1; }
                }
                Body { text: req.note; color: T.muted; font-size: 13px; }
            }
        }
    }

    component Spec inherits VerticalLayout {
        in property <string> label;
        in property <string> value;
        spacing: 2px;
        Label { text: root.label; }
        Body { text: root.value; font-size: 13px; }
    }

    export component App inherits Window {
        title: "Thallium Flash";
        preferred-width: 980px;
        preferred-height: 720px;
        min-width: 820px;
        min-height: 560px;
        background: T.base;
        default-font-family: T.human;

        in property <string> device-name;
        in property <string> device-os;
        in property <[Req]> reqs;
        in property <bool> checked;
        in property <int> verdict;   // 0 ok, 1 notes, 2 fails
        in property <[Drive]> drives;
        in-out property <int> drive-index: 0;
        in property <float> progress;
        in property <string> status: "Everything on the drive will be erased.";
        in property <bool> busy;
        in property <bool> can-customize;
        in-out property <bool> customize;

        in-out property <string> username;
        in-out property <string> full-name;
        in-out property <string> password;
        in-out property <bool> autologin;
        in-out property <string> hostname;
        in-out property <string> timezone;
        in-out property <string> locale;
        in-out property <string> keyboard;
        in-out property <string> wifi-ssid;
        in-out property <string> wifi-pass;
        in-out property <int> channel: 0;
        in-out property <int> theme: 0;
        in-out property <string> apps;
        in-out property <bool> skip-setup;

        callback run-check();
        callback flash(int);
        callback refresh();

        HorizontalLayout {
            padding: 24px;
            spacing: 20px;

            // Device card
            VerticalLayout {
                width: 280px;
                spacing: 16px;
                Plate {
                    fill: T.panel; stroke: T.line; pad: 22px; gap: 16px;
                    Label { text: "THIS PC"; }
                    Text {
                        text: root.device-name; font-family: T.brand; font-size: 22px; font-weight: 600;
                        color: T.fg; wrap: word-wrap;
                    }
                    Body { text: root.device-os; color: T.muted; font-size: 13px; }
                    Rectangle { height: 1px; background: T.line-dead; }
                    for r in root.reqs: Spec { label: r.label; value: r.value; }
                }
                Plate {
                    fill: T.panel; stroke: T.line; pad: 22px; gap: 8px;
                    Label { text: "IMAGE"; }
                    Text { text: "THALLIUM 81"; font-family: T.brand; font-size: 18px; font-weight: 600; color: T.green; }
                    Body { text: "Latest build, fetched in parallel pieces and checked against its SHA-256."; color: T.muted; font-size: 13px; }
                }
                Rectangle { vertical-stretch: 1; }
            }

            // Report
            ScrollView {
                horizontal-stretch: 1;
                VerticalLayout {
                    spacing: 14px;
                    padding-right: 14px;
                    alignment: start;

                    Plate {
                        fill: T.panel; pad: 28px; gap: 14px;
                        stroke: !root.checked ? T.line : T.level-color(root.verdict);
                        stroke-w: root.checked ? 1.4px : 1px;
                        Label { text: "COMPATIBILITY"; }
                        Text {
                            text: !root.checked ? "Can this PC run Thallium 81?"
                                : root.verdict == 0 ? "This PC is ready for Thallium 81"
                                : root.verdict == 1 ? "This PC can run Thallium 81, with notes"
                                : "This PC doesn't meet Thallium 81's requirements";
                            font-family: T.brand; font-size: 24px; font-weight: 600; wrap: word-wrap;
                            color: !root.checked ? T.fg : T.level-color(root.verdict);
                        }
                        Body {
                            text: !root.checked
                                ? "Checks the processor, memory, storage, firmware and graphics of the computer you're on. Making a drive for another PC? You can skip this."
                                : "Results for this computer. Notes won't stop the install; failures will.";
                            color: T.muted;
                        }
                        HorizontalLayout {
                            alignment: start;
                            Btn { text: root.checked ? "CHECK AGAIN" : "CHECK NOW"; primary: !root.checked; clicked => { root.run-check(); } }
                        }
                    }

                    if root.checked: VerticalLayout {
                        spacing: 8px;
                        for r in root.reqs: ReqRow { req: r; }
                    }

                    Plate {
                        fill: T.panel; stroke: T.line; pad: 28px; gap: 14px;
                        Label { text: "INSTALL DRIVE"; }
                        Text { text: "Make a Thallium USB drive"; font-family: T.brand; font-size: 20px; font-weight: 600; color: T.fg; }

                        if root.drives.length == 0: Body { text: "No USB drive found. Plug one in and refresh."; color: T.muted; }
                        for d[i] in root.drives: Rectangle {
                            height: 52px;
                            Chamfer {
                                 cut: 10px;
                                stroke: i == root.drive-index ? T.green : dtouch.has-hover ? T.line : T.line-dead;
                                fill: i == root.drive-index ? #a7c08014 : T.plate;
                            }
                            HorizontalLayout {
                                padding-left: 16px; padding-right: 16px; spacing: 14px;
                                Text { text: d.name; font-family: T.mono; font-size: 13px; font-weight: 700; vertical-alignment: center;
                                       color: i == root.drive-index ? T.green : T.fg; }
                                Body { text: d.detail; vertical-alignment: center; horizontal-stretch: 1; }
                            }
                            dtouch := TouchArea { enabled: !root.busy; mouse-cursor: pointer; clicked => { root.drive-index = i; } }
                        }
                        HorizontalLayout { alignment: start; Btn { text: "REFRESH"; enabled: !root.busy; clicked => { root.refresh(); } } }

                        if root.can-customize: Toggle {
                            text: "Customize the install (answers the installer's questions for you)";
                            checked <=> root.customize;
                            enabled: !root.busy;
                        }
                        if root.can-customize && root.customize: Plate {
                            pad: 18px; gap: 10px;
                            Body { text: "Leave anything blank and the installer asks for it."; color: T.muted; font-size: 13px; }
                            Input { label: "USERNAME"; placeholder: "e.g. neel"; text <=> root.username; }
                            Input { label: "FULL NAME"; text <=> root.full-name; }
                            Input { label: "PASSWORD"; placeholder: "stored only as a hash"; secret: true; text <=> root.password; }
                            Toggle { text: "Log in automatically"; checked <=> root.autologin; }
                            Input { label: "COMPUTER NAME"; placeholder: "e.g. thallium-laptop"; text <=> root.hostname; }
                            Input { label: "TIME ZONE"; placeholder: "e.g. Asia/Kolkata"; text <=> root.timezone; }
                            Input { label: "LANGUAGE"; placeholder: "e.g. en_IN.UTF-8"; text <=> root.locale; }
                            Input { label: "KEYBOARD"; placeholder: "e.g. us"; text <=> root.keyboard; }
                            Input { label: "WI-FI NAME"; text <=> root.wifi-ssid; }
                            Input { label: "WI-FI PASSWORD"; placeholder: "stored only as the derived key"; secret: true; text <=> root.wifi-pass; }
                            HorizontalLayout {
                                spacing: 14px;
                                Label { text: "CHANNEL"; width: 130px; vertical-alignment: center; }
                                Chip { text: "VANGUARD"; selected: root.channel == 0; clicked => { root.channel = 0; } }
                                Chip { text: "EDGE RUNNERS"; selected: root.channel == 1; clicked => { root.channel = 1; } }
                            }
                            HorizontalLayout {
                                spacing: 14px;
                                Label { text: "THEME"; width: 130px; vertical-alignment: center; }
                                Chip { text: "DARK"; selected: root.theme == 0; clicked => { root.theme = 0; } }
                                Chip { text: "LIGHT"; selected: root.theme == 1; clicked => { root.theme = 1; } }
                            }
                            Input { label: "EXTRA APPS"; placeholder: "comma separated package names"; text <=> root.apps; }
                            Toggle { text: "Skip the welcome screens on first boot"; checked <=> root.skip-setup; }
                            Body {
                                text: "This file stays readable on the USB drive. The disk to install to is still chosen in the installer.";
                                color: T.muted; font-size: 12px;
                            }
                        }

                        Rectangle {
                            height: 12px;
                            Chamfer { cut: 5px; }
                            Rectangle {
                                x: 3px; y: 3px; height: parent.height - 6px;
                                width: max(0px, (parent.width - 6px) * root.progress);
                                background: T.green;
                            }
                        }
                        Body { text: root.status; color: root.busy ? T.fg : T.muted; font-size: 13px; }
                        HorizontalLayout {
                            alignment: end;
                            Btn {
                                text: "FLASH DRIVE"; primary: true;
                                enabled: !root.busy && root.drives.length > 0;
                                clicked => { root.flash(root.drive-index); }
                            }
                        }
                    }
                }
            }
        }
    }
}
