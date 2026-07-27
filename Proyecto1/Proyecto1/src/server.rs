use std::collections::HashMap;
use std::io;
use std::net::{Shutdown, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::game::{Config, GameSession, TickEvent};
use crate::protocol::{
    decode_client_message, encode_discover_response, encode_server_message, read_framed,
    write_framed, ClientMessage, Direction, ErrorCode, GamePhase, JoinRejectReason, ServerMessage,
};

#[derive(Debug, Clone)]
pub struct ServerSnapshot {
    pub title: String,
    pub state_text: String,
    pub is_running: bool,
    pub players: Vec<(u16, String)>,
    pub tick: u32,
    pub map_size: f32,
    pub circle_radius: f32,
    pub flag_x: f32,
    pub flag_y: f32,
    pub world_players: Vec<(u16, String, f32, f32, bool)>,
    pub log_line: String,
    pub discovery_warning: Option<String>,
}

#[derive(Clone)]
pub struct ServerController {
    command_tx: Sender<ServerCommand>,
}

impl ServerController {
    pub fn start_game(&self) {
        let _ = self.command_tx.send(ServerCommand::Host(HostCommand::StartGame));
    }

    pub fn end_game(&self) {
        let _ = self.command_tx.send(ServerCommand::Host(HostCommand::EndGame));
    }
}

enum HostCommand {
    StartGame,
    EndGame,
}

enum ServerCommand {
    Host(HostCommand),
    Join {
        connection_id: u64,
        name: String,
        writer: Arc<Mutex<TcpStream>>,
    },
    Input {
        connection_id: u64,
        player_id: u16,
        direction: Direction,
    },
    Interact {
        connection_id: u64,
        player_id: u16,
    },
    Leave {
        connection_id: u64,
        player_id: u16,
    },
    ConnectionClosed {
        connection_id: u64,
        player_id: Option<u16>,
    },
}

struct ClientPeer {
    connection_id: u64,
    player_id: u16,
    writer: Arc<Mutex<TcpStream>>,
}

pub fn spawn_server(
    server_name: String,
    config: Config,
    ui_tx: Sender<ServerSnapshot>,
) -> io::Result<ServerController> {
    let session = Arc::new(Mutex::new(GameSession::new(server_name, config.clone())));
    let peers = Arc::new(Mutex::new(HashMap::<u16, ClientPeer>::new()));
    let (command_tx, command_rx) = mpsc::channel::<ServerCommand>();
    let controller = ServerController {
        command_tx: command_tx.clone(),
    };

    let discovery_warning = spawn_discovery(session.clone()).err().map(|error| {
        eprintln!(
            "Aviso: no se pudo abrir el descubrimiento UDP en el puerto {}: {}",
            config.discovery_port, error
        );
        format!(
            "Discovery no disponible en el puerto UDP {}. Compartí tu IP y puerto por TCP manualmente.",
            config.discovery_port
        )
    });
    spawn_accept_loop(config.server_port, command_tx.clone())?;
    spawn_game_loop(session, peers, command_rx, ui_tx, discovery_warning);
    Ok(controller)
}

fn spawn_discovery(session: Arc<Mutex<GameSession>>) -> io::Result<()> {
    let discovery_port = session.lock().unwrap().config.discovery_port;
    let socket = UdpSocket::bind(("0.0.0.0", discovery_port))?;
    socket.set_broadcast(true)?;
    thread::spawn(move || {
        let mut buf = [0u8; 256];
        loop {
            let Ok((size, source)) = socket.recv_from(&mut buf) else {
                continue;
            };
            if size < 2 || buf[0] != 0x01 || buf[1] != 0x03 {
                continue;
            }
            let snapshot = session.lock().unwrap().clone();
            if snapshot.state != GamePhase::Waiting {
                continue;
            }
            let response = encode_discover_response(
                snapshot.game_id,
                &snapshot.server_name,
                snapshot.config.server_port,
                snapshot.state,
                snapshot.players.len() as u16,
                snapshot.config.maximum_players,
            );
            let _ = socket.send_to(&response, source);
        }
    });
    Ok(())
}

fn spawn_accept_loop(server_port: u16, command_tx: Sender<ServerCommand>) -> io::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", server_port))?;
    let next_connection_id = Arc::new(AtomicU64::new(1));
    thread::spawn(move || {
        for incoming in listener.incoming() {
            let Ok(mut stream) = incoming else {
                continue;
            };
            let connection_id = next_connection_id.fetch_add(1, Ordering::Relaxed);
            let Ok(writer) = stream.try_clone() else {
                continue;
            };
            let writer = Arc::new(Mutex::new(writer));
            let join_tx = command_tx.clone();
            let read_tx = command_tx.clone();
            let writer_clone = writer.clone();
            thread::spawn(move || {
                let mut bound_player_id = None;
                loop {
                    let payload = match read_framed(&mut stream) {
                        Ok(payload) => payload,
                        Err(_) => {
                            let _ = read_tx.send(ServerCommand::ConnectionClosed {
                                connection_id,
                                player_id: bound_player_id,
                            });
                            break;
                        }
                    };
                    match decode_client_message(&payload) {
                        Ok(ClientMessage::Join { name }) => {
                            let _ = join_tx.send(ServerCommand::Join {
                                connection_id,
                                name,
                                writer: writer_clone.clone(),
                            });
                        }
                        Ok(ClientMessage::Input {
                            player_id,
                            direction,
                        }) => {
                            bound_player_id = Some(player_id);
                            let _ = join_tx.send(ServerCommand::Input {
                                connection_id,
                                player_id,
                                direction,
                            });
                        }
                        Ok(ClientMessage::Interact { player_id }) => {
                            bound_player_id = Some(player_id);
                            let _ = join_tx.send(ServerCommand::Interact {
                                connection_id,
                                player_id,
                            });
                        }
                        Ok(ClientMessage::Leave { player_id }) => {
                            let _ = join_tx.send(ServerCommand::Leave {
                                connection_id,
                                player_id,
                            });
                            let _ = join_tx.send(ServerCommand::ConnectionClosed {
                                connection_id,
                                player_id: Some(player_id),
                            });
                            break;
                        }
                        Err(_) => {
                            let payload = encode_server_message(&ServerMessage::Error {
                                code: ErrorCode::InvalidEncoding,
                                description: "Mensaje invalido".into(),
                            });
                            if let Ok(mut guard) = writer_clone.lock() {
                                let _ = write_framed(&mut guard, &payload);
                            }
                        }
                    }
                }
            });
        }
    });
    Ok(())
}

fn spawn_game_loop(
    session: Arc<Mutex<GameSession>>,
    peers: Arc<Mutex<HashMap<u16, ClientPeer>>>,
    command_rx: Receiver<ServerCommand>,
    ui_tx: Sender<ServerSnapshot>,
    discovery_warning: Option<String>,
) {
    thread::spawn(move || {
        let tick_duration = Duration::from_millis(session.lock().unwrap().config.tick_interval_ms as u64);
        let mut last_tick = Instant::now();
        let mut countdown_next = None::<Instant>;
        let mut countdown_remaining = 0u8;

        publish_snapshot(&session, &ui_tx, "Servidor listo", &discovery_warning);

        loop {
            while let Ok(command) = command_rx.try_recv() {
                handle_command(&session, &peers, command, &mut countdown_remaining, &mut countdown_next);
                publish_snapshot(&session, &ui_tx, "Servidor listo", &discovery_warning);
            }

            let phase = session.lock().unwrap().state;
            if phase == GamePhase::Starting {
                if countdown_next.is_none() {
                    countdown_remaining = session.lock().unwrap().config.countdown_seconds;
                    countdown_next = Some(Instant::now());
                }
                if let Some(next) = countdown_next {
                    if Instant::now() >= next {
                        broadcast(
                            &peers,
                            &ServerMessage::GameCountdown {
                                seconds_remaining: countdown_remaining,
                            },
                        );
                        publish_snapshot(
                            &session,
                            &ui_tx,
                            &format!("La partida inicia en {}s", countdown_remaining),
                            &discovery_warning,
                        );
                        if countdown_remaining == 1 {
                            let started = session.lock().unwrap().begin_running();
                            broadcast(&peers, &started);
                            countdown_next = None;
                        } else {
                            countdown_remaining = countdown_remaining.saturating_sub(1);
                            countdown_next = Some(Instant::now() + Duration::from_secs(1));
                        }
                    }
                }
            }

            if phase == GamePhase::Running && last_tick.elapsed() >= tick_duration {
                last_tick = Instant::now();
                let (events, game_state, game_over) = session.lock().unwrap().advance_tick();
                for event in events {
                    match event {
                        TickEvent::FlagPickedUp { tick, player_id } => {
                            broadcast(&peers, &ServerMessage::FlagPickedUp { tick, player_id });
                        }
                        TickEvent::FlagStolen {
                            tick,
                            previous_carrier_id,
                            new_carrier_id,
                        } => {
                            broadcast(
                                &peers,
                                &ServerMessage::FlagStolen {
                                    tick,
                                    previous_carrier_id,
                                    new_carrier_id,
                                },
                            );
                        }
                    }
                }
                broadcast(&peers, &game_state);
                if let Some(game_over) = game_over {
                    broadcast(&peers, &game_over);
                    publish_snapshot(&session, &ui_tx, "Partida finalizada", &discovery_warning);
                } else {
                    publish_snapshot(&session, &ui_tx, "Tick enviado", &discovery_warning);
                }
            }

            thread::sleep(Duration::from_millis(5));
        }
    });
}

fn handle_command(
    session: &Arc<Mutex<GameSession>>,
    peers: &Arc<Mutex<HashMap<u16, ClientPeer>>>,
    command: ServerCommand,
    countdown_remaining: &mut u8,
    countdown_next: &mut Option<Instant>,
) {
    match command {
        ServerCommand::Host(HostCommand::StartGame) => {
            let mut game = session.lock().unwrap();
            if game.start_countdown() {
                *countdown_remaining = game.config.countdown_seconds;
                *countdown_next = Some(Instant::now());
                broadcast(
                    peers,
                    &ServerMessage::LobbyState {
                        state: game.state,
                        players: game.lobby_players(),
                    },
                );
            }
        }
        ServerCommand::Host(HostCommand::EndGame) => {
            *countdown_remaining = 0;
            *countdown_next = None;

            {
                let mut game = session.lock().unwrap();
                game.state = GamePhase::Cancelled;
                broadcast(
                    peers,
                    &ServerMessage::LobbyState {
                        state: game.state,
                        players: game.lobby_players(),
                    },
                );
            }

            broadcast(
                peers,
                &ServerMessage::Error {
                    code: ErrorCode::GameFinished,
                    description: "El host finalizo la partida".into(),
                },
            );
            shutdown_all_peers(peers);
            session.lock().unwrap().reset_to_waiting();
        }
        ServerCommand::Join {
            connection_id,
            name,
            writer,
        } => {
            let trimmed = name.trim();
            let mut game = session.lock().unwrap();
            if peers
                .lock()
                .unwrap()
                .values()
                .any(|peer| peer.connection_id == connection_id)
            {
                send_to_writer(
                    &writer,
                    &ServerMessage::Error {
                        code: ErrorCode::InvalidInput,
                        description: "La conexion ya realizo JOIN".into(),
                    },
                );
                return;
            }
            if game.state != GamePhase::Waiting {
                send_to_writer(
                    &writer,
                    &ServerMessage::JoinRejected {
                        reason: JoinRejectReason::GameAlreadyStarted,
                    },
                );
                return;
            }
            if trimmed.is_empty() || trimmed.chars().count() > 20 {
                send_to_writer(
                    &writer,
                    &ServerMessage::JoinRejected {
                        reason: JoinRejectReason::InvalidName,
                    },
                );
                return;
            }
            if game.players.len() as u16 >= game.config.maximum_players {
                send_to_writer(
                    &writer,
                    &ServerMessage::JoinRejected {
                        reason: JoinRejectReason::GameFull,
                    },
                );
                return;
            }
            let player_id = game.add_player(trimmed.to_string());
            peers.lock().unwrap().insert(
                player_id,
                ClientPeer {
                    connection_id,
                    player_id,
                    writer: writer.clone(),
                },
            );
            send_to_writer(
                &writer,
                &ServerMessage::JoinAccepted {
                    player_id,
                    game_id: game.game_id,
                },
            );
            let lobby = ServerMessage::LobbyState {
                state: game.state,
                players: game.lobby_players(),
            };
            broadcast(peers, &lobby);
        }
        ServerCommand::Input {
            connection_id,
            player_id,
            direction,
        } => {
            let mut game = session.lock().unwrap();
            if !player_matches_connection(peers, connection_id, player_id) {
                return;
            }
            if game.state == GamePhase::Running {
                game.queue_input(player_id, direction);
            }
        }
        ServerCommand::Interact {
            connection_id,
            player_id,
        } => {
            let mut game = session.lock().unwrap();
            if !player_matches_connection(peers, connection_id, player_id) {
                return;
            }
            if game.state == GamePhase::Running {
                game.queue_interact(player_id);
            }
        }
        ServerCommand::Leave {
            connection_id,
            player_id,
        }
        | ServerCommand::ConnectionClosed {
            connection_id,
            player_id: Some(player_id),
        } => {
            if !player_matches_connection(peers, connection_id, player_id) {
                return;
            }
            disconnect_player(session, peers, player_id);
        }
        ServerCommand::ConnectionClosed {
            connection_id,
            player_id: None,
        } => {
            let orphaned = peers
                .lock()
                .unwrap()
                .values()
                .find(|peer| peer.connection_id == connection_id)
                .map(|peer| peer.player_id);
            if let Some(player_id) = orphaned {
                disconnect_player(session, peers, player_id);
            }
        }
    }
}

fn disconnect_player(
    session: &Arc<Mutex<GameSession>>,
    peers: &Arc<Mutex<HashMap<u16, ClientPeer>>>,
    player_id: u16,
) {
    let mut game = session.lock().unwrap();
    if game.remove_player(player_id).is_some() {
        peers.lock().unwrap().remove(&player_id);
        broadcast(peers, &ServerMessage::PlayerDisconnected { player_id });
        broadcast(
            peers,
            &ServerMessage::LobbyState {
                state: game.state,
                players: game.lobby_players(),
            },
        );
    }
}

fn send_to_writer(writer: &Arc<Mutex<TcpStream>>, message: &ServerMessage) {
    let payload = encode_server_message(message);
    if let Ok(mut guard) = writer.lock() {
        let _ = write_framed(&mut guard, &payload);
    }
}

fn broadcast(peers: &Arc<Mutex<HashMap<u16, ClientPeer>>>, message: &ServerMessage) {
    let payload = encode_server_message(message);
    let peers_snapshot: Vec<Arc<Mutex<TcpStream>>> = peers
        .lock()
        .unwrap()
        .values()
        .map(|peer| peer.writer.clone())
        .collect();
    for writer in peers_snapshot {
        if let Ok(mut guard) = writer.lock() {
            let _ = write_framed(&mut guard, &payload);
        }
    }
}

fn publish_snapshot(
    session: &Arc<Mutex<GameSession>>,
    ui_tx: &Sender<ServerSnapshot>,
    log_line: &str,
    discovery_warning: &Option<String>,
) {
    let game = session.lock().unwrap().clone();
    let snapshot = ServerSnapshot {
        title: game.server_name.clone(),
        state_text: format!("{:?}", game.state),
        is_running: matches!(game.state, GamePhase::Running | GamePhase::Finished),
        players: game
            .players
            .values()
            .map(|player| (player.player_id, player.name.clone()))
            .collect(),
        tick: game.tick,
        map_size: game.config.map_size,
        circle_radius: game.config.circle_radius,
        flag_x: game.flag.x,
        flag_y: game.flag.y,
        world_players: if matches!(game.state, GamePhase::Running | GamePhase::Finished) {
            game.players
                .values()
                .map(|player| {
                    (
                        player.player_id,
                        player.name.clone(),
                        player.x,
                        player.y,
                        player.has_flag,
                    )
                })
                .collect()
        } else {
            Vec::new()
        },
        log_line: log_line.to_string(),
        discovery_warning: discovery_warning.clone(),
    };
    let _ = ui_tx.send(snapshot);
}

fn player_matches_connection(
    peers: &Arc<Mutex<HashMap<u16, ClientPeer>>>,
    connection_id: u64,
    player_id: u16,
) -> bool {
    peers.lock().unwrap().get(&player_id).is_some_and(|peer| {
        peer.player_id == player_id && peer.connection_id == connection_id
    })
}

fn shutdown_all_peers(peers: &Arc<Mutex<HashMap<u16, ClientPeer>>>) {
    let peers_snapshot: Vec<Arc<Mutex<TcpStream>>> = peers
        .lock()
        .unwrap()
        .values()
        .map(|peer| peer.writer.clone())
        .collect();
    for writer in peers_snapshot {
        if let Ok(guard) = writer.lock() {
            let _ = guard.shutdown(Shutdown::Both);
        }
    }
    peers.lock().unwrap().clear();
}
