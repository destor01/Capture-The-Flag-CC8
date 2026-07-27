use std::collections::HashMap;
use std::io;
use std::net::{TcpStream, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::protocol::{
    decode_server_message, encode_client_message, read_discover_packet, read_framed,
    wire_to_world, write_discover_request, write_framed, ClientMessage, Direction,
    DiscoverResponse, FlagStatus, LobbyPlayer, PlayerWire, ServerMessage,
};

#[derive(Debug, Clone)]
pub struct ClientSnapshot {
    pub title: String,
    pub status: String,
    pub player_id: u16,
    pub lobby_players: Vec<String>,
    pub discovered_servers: Vec<(String, String)>,
    pub map_size: f32,
    pub circle_radius: f32,
    pub flag_x: f32,
    pub flag_y: f32,
    pub tick: u32,
    pub world_players: Vec<(u16, String, f32, f32, bool)>,
    pub log_line: String,
    pub is_running: bool,
}

pub enum UiToClient {
    Discover {
        port: u16,
    },
    Connect {
        name: String,
        host: String,
        port: u16,
    },
    Input(Direction),
    Interact,
    Leave,
}

pub fn spawn_client(
    ui_tx: Sender<ClientSnapshot>,
) -> Sender<UiToClient> {
    let (tx, rx) = mpsc::channel::<UiToClient>();
    thread::spawn(move || run_client_loop(rx, ui_tx));
    tx
}

struct RuntimeState {
    title: String,
    status: String,
    player_id: u16,
    map_size: f32,
    circle_radius: f32,
    flag_x: f32,
    flag_y: f32,
    tick: u32,
    lobby_players: Vec<LobbyPlayer>,
    players: HashMap<u16, (String, f32, f32, bool)>,
    discovered: Vec<DiscoverResponse>,
    writer: Option<Arc<Mutex<TcpStream>>>,
    is_running: bool,
}

enum NetworkEvent {
    Message(ServerMessage),
    Disconnected,
}

fn run_client_loop(rx: Receiver<UiToClient>, ui_tx: Sender<ClientSnapshot>) {
    let mut state = RuntimeState {
        title: "Jugador".into(),
        status: "Listo para descubrir servidores".into(),
        player_id: 0,
        map_size: 2000.0,
        circle_radius: 500.0,
        flag_x: 0.0,
        flag_y: 0.0,
        tick: 0,
        lobby_players: Vec::new(),
        players: HashMap::new(),
        discovered: Vec::new(),
        writer: None,
        is_running: false,
    };

    let (network_tx, network_rx) = mpsc::channel::<NetworkEvent>();
    let mut last_publish = Instant::now();

    loop {
        while let Ok(command) = rx.try_recv() {
            match command {
                UiToClient::Discover { port } => {
                    state.discovered = discover_servers(port).unwrap_or_default();
                    state.status = if state.discovered.is_empty() {
                        "No se encontraron servidores".into()
                    } else {
                        format!("Se encontraron {} servidores", state.discovered.len())
                    };
                }
                UiToClient::Connect { name, host, port } => {
                    if state.writer.is_some() {
                        state.status = "Ya existe una conexion activa. Usa Salir antes de volver a conectar.".into();
                        continue;
                    }
                    let connect_result = connect_to_server(&name, &host, port, network_tx.clone());
                    match connect_result {
                        Ok(writer) => {
                            state.title = format!("Jugador: {}", name);
                            state.status = format!("Conectado a {}:{}", host, port);
                            state.writer = Some(writer);
                        }
                        Err(error) => {
                            state.status = format!("No se pudo conectar: {}", error);
                        }
                    }
                }
                UiToClient::Input(direction) => {
                    if let Some(writer) = &state.writer {
                        let payload = encode_client_message(&ClientMessage::Input {
                            player_id: state.player_id,
                            direction,
                        });
                        if let Ok(mut guard) = writer.lock() {
                            let _ = write_framed(&mut guard, &payload);
                        }
                    }
                }
                UiToClient::Interact => {
                    if let Some(writer) = &state.writer {
                        let payload = encode_client_message(&ClientMessage::Interact {
                            player_id: state.player_id,
                        });
                        if let Ok(mut guard) = writer.lock() {
                            let _ = write_framed(&mut guard, &payload);
                        }
                    }
                }
                UiToClient::Leave => {
                    if let Some(writer) = &state.writer {
                        let payload = encode_client_message(&ClientMessage::Leave {
                            player_id: state.player_id,
                        });
                        if let Ok(mut guard) = writer.lock() {
                            let _ = write_framed(&mut guard, &payload);
                        }
                    }
                    state.writer = None;
                    state.player_id = 0;
                    state.tick = 0;
                    state.players.clear();
                    state.lobby_players.clear();
                    state.is_running = false;
                    state.status = "Conexion cerrada".into();
                }
            }
        }

        while let Ok(event) = network_rx.try_recv() {
            match event {
                NetworkEvent::Message(message) => apply_server_message(&mut state, message),
                NetworkEvent::Disconnected => {
                    state.writer = None;
                    state.player_id = 0;
                    state.tick = 0;
                    state.players.clear();
                    state.lobby_players.clear();
                    state.status = "El servidor cerro la sesion".into();
                }
            }
        }

        if last_publish.elapsed() >= Duration::from_millis(30) {
            publish(&state, &ui_tx);
            last_publish = Instant::now();
        }

        thread::sleep(Duration::from_millis(5));
    }
}

fn discover_servers(port: u16) -> io::Result<Vec<DiscoverResponse>> {
    let socket = UdpSocket::bind(("0.0.0.0", 0))?;
    socket.set_broadcast(true)?;
    socket.set_read_timeout(Some(Duration::from_millis(250)))?;
    write_discover_request(&socket, port)?;

    let mut responses = Vec::new();
    let started = Instant::now();
    let mut buf = [0u8; 256];
    let mut seen = std::collections::HashSet::new();
    while started.elapsed() < Duration::from_millis(900) {
        if let Ok((size, source)) = socket.recv_from(&mut buf) {
            if let Some(response) = read_discover_packet(&buf[..size], source)? {
                let key = (response.game_id, response.source.ip(), response.tcp_port);
                if seen.insert(key) {
                    responses.push(response);
                }
            }
        }
    }
    Ok(responses)
}

fn connect_to_server(
    name: &str,
    host: &str,
    port: u16,
    network_tx: Sender<NetworkEvent>,
) -> io::Result<Arc<Mutex<TcpStream>>> {
    let mut stream = TcpStream::connect((host, port))?;
    stream.set_nodelay(true)?;
    let writer = Arc::new(Mutex::new(stream.try_clone()?));
    let join_payload = encode_client_message(&ClientMessage::Join {
        name: name.to_string(),
    });
    write_framed(&mut stream, &join_payload)?;
    thread::spawn(move || {
        let mut stream = stream;
        while let Ok(payload) = read_framed(&mut stream) {
            if let Ok(message) = decode_server_message(&payload) {
                let _ = network_tx.send(NetworkEvent::Message(message));
            }
        }
        let _ = network_tx.send(NetworkEvent::Disconnected);
    });
    Ok(writer)
}

fn apply_server_message(state: &mut RuntimeState, message: ServerMessage) {
    match message {
        ServerMessage::JoinAccepted { player_id, game_id } => {
            state.player_id = player_id;
            state.status = format!("JOIN aceptado. gameId={} playerId={}", game_id, player_id);
        }
        ServerMessage::JoinRejected { reason } => {
            state.status = format!("JOIN rechazado: {:?}", reason);
            state.writer = None;
            state.player_id = 0;
        }
        ServerMessage::LobbyState { state: phase, players } => {
            state.status = format!("Lobby: {:?}", phase);
            state.lobby_players = players;
            state.is_running = false;
        }
        ServerMessage::GameCountdown { seconds_remaining } => {
            state.status = format!("La partida inicia en {}s", seconds_remaining);
        }
        ServerMessage::GameStarted {
            map_size,
            circle_radius,
            flag_x,
            flag_y,
            players,
            ..
        } => {
            state.map_size = wire_to_world(map_size);
            state.circle_radius = wire_to_world(circle_radius);
            state.flag_x = wire_to_world(flag_x);
            state.flag_y = wire_to_world(flag_y);
            state.tick = 0;
            state.is_running = true;
            state.players = players
                .into_iter()
                .map(player_wire_to_entry)
                .map(|(id, name, x, y, has_flag)| (id, (name, x, y, has_flag)))
                .collect();
            state.status = "Partida iniciada".into();
        }
        ServerMessage::GameState {
            tick,
            flag_status,
            flag_carrier_id,
            flag_x,
            flag_y,
            players,
        } => {
            if tick >= state.tick {
                state.tick = tick;
                state.flag_x = wire_to_world(flag_x);
                state.flag_y = wire_to_world(flag_y);
                for player in players {
                    let entry = state.players.entry(player.player_id).or_insert_with(|| {
                        (
                            format!("P{}", player.player_id),
                            wire_to_world(player.x),
                            wire_to_world(player.y),
                            player.has_flag,
                        )
                    });
                    entry.1 = wire_to_world(player.x);
                    entry.2 = wire_to_world(player.y);
                    entry.3 = player.has_flag;
                }
                state.status = match flag_status {
                    FlagStatus::Carried => format!("Tick {}. Bandera en manos de {}", tick, flag_carrier_id),
                    _ => format!("Tick {}", tick),
                };
            }
        }
        ServerMessage::FlagPickedUp { player_id, .. } => {
            state.status = format!("Jugador {} tomo la bandera", player_id);
        }
        ServerMessage::FlagStolen {
            previous_carrier_id,
            new_carrier_id,
            ..
        } => {
            state.status = format!(
                "Bandera robada: {} -> {}",
                previous_carrier_id, new_carrier_id
            );
        }
        ServerMessage::PlayerDisconnected { player_id } => {
            state.players.remove(&player_id);
            state.status = format!("Jugador {} se desconecto", player_id);
        }
        ServerMessage::GameOver {
            winner_id,
            winner_name,
            ..
        } => {
            state.status = format!("Partida terminada. Gana {} ({})", winner_name, winner_id);
            state.is_running = false;
        }
        ServerMessage::Error { code, description } => {
            state.status = format!("ERROR {:?}: {}", code, description);
        }
    }
}

fn player_wire_to_entry(player: PlayerWire) -> (u16, String, f32, f32, bool) {
    (
        player.player_id,
        player.name,
        wire_to_world(player.x),
        wire_to_world(player.y),
        player.has_flag,
    )
}

fn publish(state: &RuntimeState, ui_tx: &Sender<ClientSnapshot>) {
    let snapshot = ClientSnapshot {
        title: state.title.clone(),
        status: state.status.clone(),
        player_id: state.player_id,
        lobby_players: state.lobby_players.iter().map(|player| player.name.clone()).collect(),
        discovered_servers: state
            .discovered
            .iter()
            .map(|server| {
                (
                    format!(
                        "{} ({:?}) {} / {}",
                        server.server_name, server.state, server.player_count, server.maximum_players
                    ),
                    format!("{}:{}", server.source.ip(), server.tcp_port),
                )
            })
            .collect(),
        map_size: state.map_size,
        circle_radius: state.circle_radius,
        flag_x: state.flag_x,
        flag_y: state.flag_y,
        tick: state.tick,
        world_players: state
            .players
            .iter()
            .map(|(id, (name, x, y, has_flag))| (*id, name.clone(), *x, *y, *has_flag))
            .collect(),
        log_line: state.status.clone(),
        is_running: state.is_running,
    };
    let _ = ui_tx.send(snapshot);
}
