mod client;
mod game;
mod protocol;
mod server;

use std::env;
use std::sync::mpsc;

use macroquad::prelude::*;

use client::{spawn_client, ClientSnapshot, UiToClient};
use game::Config;
use protocol::Direction;
use server::{spawn_server, ServerSnapshot};

const MAP_BOX: f32 = 480.0;

fn window_conf() -> Conf {
    let mode = env::args().nth(1).unwrap_or_else(|| "client".to_string());
    let (title, width, height) = match mode.as_str() {
        "server" => ("Captura la Bandera - Servidor", 960, 640),
        _ => ("Captura la Bandera - Cliente", 1100, 680),
    };
    Conf {
        window_title: title.to_string(),
        window_width: width,
        window_height: height,
        window_resizable: false,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| "client".to_string());
    match mode.as_str() {
        "server" => run_server_ui().await,
        "client" => run_client_ui().await,
        other => eprintln!("Modo desconocido: {}. Usa `server` o `client`.", other),
    }
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0)
}

fn point_in_rect(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32) -> bool {
    px >= x && px <= x + w && py >= y && py <= y + h
}

fn button(x: f32, y: f32, w: f32, h: f32, label: &str) -> bool {
    let (mx, my) = mouse_position();
    let hovered = point_in_rect(mx, my, x, y, w, h);
    let fill = if hovered { rgba(0x2a, 0x9d, 0x8f, 255) } else { rgba(0x21, 0x9e, 0xbc, 255) };
    draw_rectangle(x, y, w, h, fill);
    draw_rectangle_lines(x, y, w, h, 2.0, WHITE);
    let dims = measure_text(label, None, 18, 1.0);
    draw_text(label, x + (w - dims.width) / 2.0, y + h / 2.0 + dims.height / 2.0, 18.0, WHITE);
    hovered && is_mouse_button_pressed(MouseButton::Left)
}

#[derive(Default)]
struct TextField {
    value: String,
    active: bool,
}

impl TextField {
    fn new(initial: &str) -> Self {
        Self { value: initial.to_string(), active: false }
    }

    fn update_and_draw(&mut self, x: f32, y: f32, w: f32, h: f32, placeholder: &str) {
        let (mx, my) = mouse_position();
        let hovered = point_in_rect(mx, my, x, y, w, h);
        if is_mouse_button_pressed(MouseButton::Left) {
            self.active = hovered;
        }
        if self.active {
            while let Some(c) = get_char_pressed() {
                if !c.is_control() && self.value.chars().count() < 24 {
                    self.value.push(c);
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                self.value.pop();
            }
        }
        let border = if self.active { rgba(0xff, 0xd6, 0xa5, 255) } else { WHITE };
        draw_rectangle(x, y, w, h, rgba(0x14, 0x14, 0x28, 255));
        draw_rectangle_lines(x, y, w, h, 2.0, border);
        if self.value.is_empty() {
            draw_text(placeholder, x + 6.0, y + h / 2.0 + 6.0, 18.0, GRAY);
        } else {
            draw_text(&self.value, x + 6.0, y + h / 2.0 + 6.0, 18.0, WHITE);
        }
    }
}

fn palette(index: usize) -> Color {
    const COLORS: [(u8, u8, u8); 5] = [
        (0xE6, 0x39, 0x46),
        (0x45, 0x7B, 0x9D),
        (0x2A, 0x9D, 0x8F),
        (0xF4, 0xA2, 0x61),
        (0x8D, 0x99, 0xAE),
    ];
    let (r, g, b) = COLORS[index % COLORS.len()];
    rgba(r, g, b, 255)
}

fn scale_world(value: f32, map_size: f32, box_size: f32) -> f32 {
    let half = map_size / 2.0;
    ((value + half) / map_size) * box_size
}

fn scale_size(value: f32, map_size: f32, box_size: f32) -> f32 {
    (value / map_size) * box_size
}

fn draw_shared_map(
    ox: f32,
    oy: f32,
    box_size: f32,
    players: &[(String, f32, f32, bool, usize)],
    flag_x: f32,
    flag_y: f32,
    circle_size: f32,
) {
    draw_rectangle(ox, oy, box_size, box_size, rgba(0x0f, 0x17, 0x2a, 255));
    draw_rectangle_lines(ox, oy, box_size, box_size, 1.0, rgba(0x8e, 0xca, 0xe6, 255));

    let center = box_size / 2.0;
    draw_circle(ox + center, oy + center, circle_size / 2.0, rgba(0x21, 0x9e, 0xbc, 50));
    draw_circle_lines(ox + center, oy + center, circle_size / 2.0, 2.0, rgba(0x21, 0x9e, 0xbc, 255));

    draw_circle(ox + flag_x, oy + flag_y, 6.0, rgba(0xff, 0xb7, 0x03, 255));

    for (name, x, y, has_flag, index) in players {
        let color = if *has_flag { rgba(255, 180, 0, 255) } else { palette(*index) };
        draw_circle(ox + x, oy + y, 8.0, color);
        let dims = measure_text(name, None, 14, 1.0);
        draw_text(name, ox + x - dims.width / 2.0, oy + y + 22.0, 14.0, WHITE);
    }
}

async fn run_server_ui() {
    let mut name_field = TextField::new("");
    let mut setup_error: Option<String> = None;

    loop {
        clear_background(rgba(0x1d, 0x35, 0x57, 255));
        draw_text("Nueva partida de Captura la Bandera", 30.0, 60.0, 26.0, WHITE);
        draw_text("Nombre del servidor", 30.0, 100.0, 16.0, rgba(0xca, 0xf0, 0xf8, 255));
        name_field.update_and_draw(30.0, 116.0, 340.0, 36.0, "Servidor sin nombre");
        if let Some(error) = &setup_error {
            draw_text(error, 30.0, 176.0, 15.0, rgba(0xe6, 0x39, 0x46, 255));
        }
        let start_clicked = button(30.0, 196.0, 200.0, 40.0, "Crear servidor")
            || is_key_pressed(KeyCode::Enter);

        if start_clicked {
            let trimmed = name_field.value.trim();
            let server_name = if trimmed.is_empty() {
                "Servidor sin nombre".to_string()
            } else {
                trimmed.chars().take(30).collect::<String>()
            };
            let (snapshot_tx, snapshot_rx) = mpsc::channel::<ServerSnapshot>();
            match spawn_server(server_name, Config::default(), snapshot_tx) {
                Ok(controller) => {
                    run_server_dashboard(controller, snapshot_rx).await;
                    return;
                }
                Err(error) => {
                    setup_error = Some(format!("No se pudo levantar el servidor: {}", error));
                }
            }
        }

        next_frame().await;
    }
}

async fn run_server_dashboard(
    controller: server::ServerController,
    snapshot_rx: mpsc::Receiver<ServerSnapshot>,
) {
    let mut title = "Servidor".to_string();
    let mut state_text = "WAITING".to_string();
    let mut tick_text = "Tick 0".to_string();
    let mut log_text = String::new();
    let mut lobby: Vec<String> = Vec::new();
    let mut map_size = 2000.0f32;
    let mut circle_radius = 500.0f32;
    let mut flag_x = 0.0f32;
    let mut flag_y = 0.0f32;
    let mut world_players: Vec<(u16, String, f32, f32, bool)> = Vec::new();
    let mut discovery_warning: Option<String> = None;

    loop {
        while let Ok(snapshot) = snapshot_rx.try_recv() {
            title = snapshot.title;
            state_text = snapshot.state_text;
            tick_text = format!("Tick {}", snapshot.tick);
            log_text = snapshot.log_line;
            map_size = snapshot.map_size;
            circle_radius = snapshot.circle_radius;
            flag_x = snapshot.flag_x;
            flag_y = snapshot.flag_y;
            lobby = snapshot
                .players
                .iter()
                .map(|(id, name)| format!("P{} - {}", id, name))
                .collect();
            world_players = if snapshot.is_running { snapshot.world_players } else { Vec::new() };
            discovery_warning = snapshot.discovery_warning;
        }

        clear_background(rgba(0x1d, 0x35, 0x57, 255));

        draw_text(&title, 30.0, 44.0, 30.0, WHITE);
        draw_text(&state_text, 30.0, 70.0, 18.0, rgba(0xd9, 0xed, 0x92, 255));
        draw_text(&tick_text, 30.0, 92.0, 15.0, rgba(0xca, 0xf0, 0xf8, 255));

        let map_x = 30.0;
        let map_y = 110.0;
        let players_draw: Vec<(String, f32, f32, bool, usize)> = world_players
            .iter()
            .enumerate()
            .map(|(index, (_id, name, x, y, has_flag))| {
                (
                    name.clone(),
                    scale_world(*x, map_size, MAP_BOX),
                    scale_world(*y, map_size, MAP_BOX),
                    *has_flag,
                    index,
                )
            })
            .collect();
        draw_shared_map(
            map_x,
            map_y,
            MAP_BOX,
            &players_draw,
            scale_world(flag_x, map_size, MAP_BOX),
            scale_world(flag_y, map_size, MAP_BOX),
            scale_size(circle_radius * 2.0, map_size, MAP_BOX),
        );

        let panel_x = 560.0;
        if button(panel_x, 40.0, 240.0, 44.0, "Iniciar partida") {
            controller.start_game();
        }
        if button(panel_x, 96.0, 240.0, 44.0, "Finalizar partida") {
            controller.end_game();
        }
        draw_text("Jugadores conectados", panel_x, 165.0, 20.0, rgba(0xca, 0xf0, 0xf8, 255));
        for (i, label) in lobby.iter().enumerate() {
            draw_text(label, panel_x, 192.0 + i as f32 * 22.0, 16.0, WHITE);
        }

        if let Some(warning) = &discovery_warning {
            let box_y = 400.0;
            draw_rectangle(panel_x, box_y, 340.0, 64.0, rgba(0x4a, 0x1d, 0x1d, 220));
            draw_rectangle_lines(panel_x, box_y, 340.0, 64.0, 2.0, rgba(0xe6, 0x39, 0x46, 255));
            match warning.split_once(". ") {
                Some((first, second)) => {
                    draw_text(&format!("{}.", first), panel_x + 10.0, box_y + 26.0, 15.0, rgba(0xff, 0xb4, 0xa2, 255));
                    draw_text(second, panel_x + 10.0, box_y + 48.0, 15.0, rgba(0xff, 0xb4, 0xa2, 255));
                }
                None => {
                    draw_text(warning, panel_x + 10.0, box_y + 36.0, 15.0, rgba(0xff, 0xb4, 0xa2, 255));
                }
            }
        }

        draw_text(&log_text, panel_x, 600.0, 16.0, rgba(0xff, 0xd6, 0xa5, 255));

        next_frame().await;
    }
}

async fn run_client_ui() {
    let (snapshot_tx, snapshot_rx) = mpsc::channel::<ClientSnapshot>();
    let client_tx = spawn_client(snapshot_tx);

    let mut name_field = TextField::new("Jugador1");
    let mut host_field = TextField::new("127.0.0.1");
    let mut port_field = TextField::new("5000");

    let mut snapshot = ClientSnapshot {
        title: "Jugador".into(),
        status: "Listo para descubrir servidores".into(),
        player_id: 0,
        lobby_players: Vec::new(),
        discovered_servers: Vec::new(),
        map_size: 2000.0,
        circle_radius: 500.0,
        flag_x: 0.0,
        flag_y: 0.0,
        tick: 0,
        world_players: Vec::new(),
        log_line: String::new(),
        is_running: false,
    };
    let mut last_direction = Direction::None;

    loop {
        while let Ok(latest) = snapshot_rx.try_recv() {
            snapshot = latest;
        }

        let connected = snapshot.player_id != 0;

        if connected && snapshot.is_running {
            let direction = if is_key_down(KeyCode::Up) || is_key_down(KeyCode::W) {
                Direction::Up
            } else if is_key_down(KeyCode::Down) || is_key_down(KeyCode::S) {
                Direction::Down
            } else if is_key_down(KeyCode::Left) || is_key_down(KeyCode::A) {
                Direction::Left
            } else if is_key_down(KeyCode::Right) || is_key_down(KeyCode::D) {
                Direction::Right
            } else {
                Direction::None
            };
            if direction != last_direction {
                let _ = client_tx.send(UiToClient::Input(direction));
                last_direction = direction;
            }
            if is_key_pressed(KeyCode::E) {
                let _ = client_tx.send(UiToClient::Interact);
            }
        }
        if connected && is_key_pressed(KeyCode::Escape) {
            let _ = client_tx.send(UiToClient::Leave);
            last_direction = Direction::None;
        }

        clear_background(rgba(0x0b, 0x13, 0x2b, 255));

        draw_text(&snapshot.title, 30.0, 44.0, 26.0, WHITE);
        draw_text(&snapshot.status, 30.0, 70.0, 16.0, rgba(0xca, 0xff, 0xbf, 255));
        draw_text(&format!("PlayerId {}", snapshot.player_id), 30.0, 92.0, 14.0, rgba(0xad, 0xe8, 0xf4, 255));
        draw_text(&format!("Tick {}", snapshot.tick), 30.0, 112.0, 14.0, rgba(0xad, 0xe8, 0xf4, 255));

        let mut y = 140.0;
        if !connected {
            name_field.update_and_draw(30.0, y, 220.0, 32.0, "Nombre");
            if button(260.0, y, 90.0, 32.0, "Buscar") {
                let _ = client_tx.send(UiToClient::Discover { port: protocol::DISCOVERY_PORT });
            }
            y += 44.0;
            host_field.update_and_draw(30.0, y, 160.0, 32.0, "Host");
            port_field.update_and_draw(200.0, y, 90.0, 32.0, "Puerto");
            y += 44.0;
            if button(30.0, y, 260.0, 36.0, "Conectar") {
                if let Ok(port) = port_field.value.trim().parse::<u16>() {
                    let _ = client_tx.send(UiToClient::Connect {
                        name: name_field.value.trim().to_string(),
                        host: host_field.value.trim().to_string(),
                        port,
                    });
                }
            }
            y += 50.0;
            draw_text("Servidores encontrados", 30.0, y, 18.0, WHITE);
            y += 14.0;
            for (label, address) in &snapshot.discovered_servers {
                y += 30.0;
                if button(30.0, y, 320.0, 28.0, label) {
                    if let Some((host, port)) = address.rsplit_once(':') {
                        host_field.value = host.to_string();
                        port_field.value = port.to_string();
                    }
                }
            }
        } else {
            draw_text("Lobby", 30.0, y, 18.0, WHITE);
            y += 24.0;
            for player in &snapshot.lobby_players {
                draw_text(player, 30.0, y, 16.0, WHITE);
                y += 22.0;
            }
            y += 20.0;
            draw_text(
                "Flechas o WASD para mover. E captura o roba la bandera. Esc para salir.",
                30.0,
                y,
                14.0,
                rgba(0xcf, 0xe8, 0xff, 255),
            );
            if button(30.0, 600.0, 160.0, 34.0, "Salir") {
                let _ = client_tx.send(UiToClient::Leave);
                last_direction = Direction::None;
            }
        }

        let map_x = 400.0;
        let map_y = 140.0;
        let players_draw: Vec<(String, f32, f32, bool, usize)> = snapshot
            .world_players
            .iter()
            .enumerate()
            .map(|(index, (_id, name, x, y, has_flag))| {
                (
                    name.clone(),
                    scale_world(*x, snapshot.map_size, MAP_BOX),
                    scale_world(*y, snapshot.map_size, MAP_BOX),
                    *has_flag,
                    index,
                )
            })
            .collect();
        draw_shared_map(
            map_x,
            map_y,
            MAP_BOX,
            &players_draw,
            scale_world(snapshot.flag_x, snapshot.map_size, MAP_BOX),
            scale_world(snapshot.flag_y, snapshot.map_size, MAP_BOX),
            scale_size(snapshot.circle_radius * 2.0, snapshot.map_size, MAP_BOX),
        );

        draw_text(&snapshot.log_line, 30.0, 660.0, 14.0, rgba(0xff, 0xd6, 0xa5, 255));

        next_frame().await;
    }
}
