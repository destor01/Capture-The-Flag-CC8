use std::collections::HashSet;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket};

pub const PROTOCOL_VERSION: u8 = 3;

/// Puerto UDP de discovery. El PRFC-CC8-2026 (§21, §36) fija este valor en
/// 5001 para que todas las implementaciones del curso se puedan descubrir
/// entre si. Si en una maquina de desarrollo especifica ese puerto esta
/// ocupado por otro software, usar conexion manual por IP:puerto en vez de
/// cambiar esta constante: cambiarla rompe el discovery UDP automatico con
/// las implementaciones de los demas grupos.
pub const DISCOVERY_PORT: u16 = 5001;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessageType {
    DiscoverRequest = 0x01,
    DiscoverResponse = 0x02,
    Join = 0x10,
    Input = 0x11,
    Interact = 0x12,
    Leave = 0x13,
    JoinAccepted = 0x20,
    JoinRejected = 0x21,
    LobbyState = 0x22,
    GameCountdown = 0x23,
    GameStarted = 0x24,
    GameState = 0x25,
    FlagPickedUp = 0x26,
    FlagStolen = 0x27,
    PlayerDisconnected = 0x28,
    GameOver = 0x29,
    Error = 0x2A,
}

impl MessageType {
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0x01 => Self::DiscoverRequest,
            0x02 => Self::DiscoverResponse,
            0x10 => Self::Join,
            0x11 => Self::Input,
            0x12 => Self::Interact,
            0x13 => Self::Leave,
            0x20 => Self::JoinAccepted,
            0x21 => Self::JoinRejected,
            0x22 => Self::LobbyState,
            0x23 => Self::GameCountdown,
            0x24 => Self::GameStarted,
            0x25 => Self::GameState,
            0x26 => Self::FlagPickedUp,
            0x27 => Self::FlagStolen,
            0x28 => Self::PlayerDisconnected,
            0x29 => Self::GameOver,
            0x2A => Self::Error,
            _ => return None,
        })
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamePhase {
    Waiting = 0x01,
    Starting = 0x02,
    Running = 0x03,
    Finished = 0x04,
    Cancelled = 0x05,
}

impl GamePhase {
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0x01 => Self::Waiting,
            0x02 => Self::Starting,
            0x03 => Self::Running,
            0x04 => Self::Finished,
            0x05 => Self::Cancelled,
            _ => return None,
        })
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagStatus {
    Available = 0x01,
    Carried = 0x02,
    Dropped = 0x03,
    Outside = 0x04,
}

impl FlagStatus {
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0x01 => Self::Available,
            0x02 => Self::Carried,
            0x03 => Self::Dropped,
            0x04 => Self::Outside,
            _ => return None,
        })
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    None = 0x00,
    Up = 0x01,
    Down = 0x02,
    Left = 0x03,
    Right = 0x04,
}

impl Direction {
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0x00 => Self::None,
            0x01 => Self::Up,
            0x02 => Self::Down,
            0x03 => Self::Left,
            0x04 => Self::Right,
            _ => return None,
        })
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinRejectReason {
    GameAlreadyStarted = 0x01,
    GameFull = 0x02,
    InvalidName = 0x03,
    UnsupportedProtocolVersion = 0x04,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameOverReason {
    ExitedCircleWithFlag = 0x01,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidMessage = 0x01,
    InvalidEncoding = 0x02,
    InvalidInput = 0x03,
    UnknownPlayer = 0x04,
    GameNotStarted = 0x05,
    GameAlreadyStarted = 0x06,
    GameFinished = 0x07,
    UnsupportedProtocolVersion = 0x08,
}

#[derive(Debug, Clone)]
pub struct LobbyPlayer {
    pub player_id: u16,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct PlayerWire {
    pub player_id: u16,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub direction: Direction,
    pub has_flag: bool,
}

#[derive(Debug, Clone)]
pub struct PlayerStateWire {
    pub player_id: u16,
    pub x: i32,
    pub y: i32,
    pub direction: Direction,
    pub has_flag: bool,
}

#[derive(Debug, Clone)]
pub struct DiscoverResponse {
    pub game_id: u16,
    pub server_name: String,
    pub tcp_port: u16,
    pub state: GamePhase,
    pub player_count: u16,
    pub maximum_players: u16,
    pub source: SocketAddr,
}

#[derive(Debug, Clone)]
pub enum ClientMessage {
    Join { name: String },
    Input { player_id: u16, direction: Direction },
    Interact { player_id: u16 },
    Leave { player_id: u16 },
}

#[derive(Debug, Clone)]
pub enum ServerMessage {
    JoinAccepted { player_id: u16, game_id: u16 },
    JoinRejected { reason: JoinRejectReason },
    LobbyState { state: GamePhase, players: Vec<LobbyPlayer> },
    GameCountdown { seconds_remaining: u8 },
    GameStarted {
        map_size: i32,
        circle_radius: i32,
        player_radius: i32,
        player_speed: i32,
        interaction_radius: i32,
        tick_interval_ms: u16,
        flag_status: FlagStatus,
        flag_carrier_id: u16,
        flag_x: i32,
        flag_y: i32,
        players: Vec<PlayerWire>,
    },
    GameState {
        tick: u32,
        flag_status: FlagStatus,
        flag_carrier_id: u16,
        flag_x: i32,
        flag_y: i32,
        players: Vec<PlayerStateWire>,
    },
    FlagPickedUp { tick: u32, player_id: u16 },
    FlagStolen { tick: u32, previous_carrier_id: u16, new_carrier_id: u16 },
    PlayerDisconnected { player_id: u16 },
    GameOver { winner_id: u16, winner_name: String, reason: GameOverReason },
    Error { code: ErrorCode, description: String },
}

pub fn world_to_wire(value: f32) -> i32 {
    (value * 100.0).round() as i32
}

pub fn wire_to_world(value: i32) -> f32 {
    value as f32 / 100.0
}

/// Direcciones de broadcast a las que vale la pena mandar el DISCOVER_REQUEST:
/// el broadcast global (255.255.255.255) mas el broadcast dirigido de cada
/// interfaz IPv4 activa (ip | !netmask). Mandar el broadcast dirigido de cada
/// interfaz es lo que permite llegar a la red de RadminVPN incluso cuando esa
/// interfaz no es la ruta por defecto del sistema operativo: al ser una
/// direccion que solo pertenece a la subred de esa interfaz, Windows la
/// enruta por el adaptador correcto sin ambiguedad.
fn discovery_broadcast_targets() -> Vec<Ipv4Addr> {
    let mut targets = HashSet::new();
    targets.insert(Ipv4Addr::new(255, 255, 255, 255));

    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for interface in interfaces {
            if interface.is_loopback() {
                continue;
            }
            if let if_addrs::IfAddr::V4(v4) = interface.addr {
                if v4.netmask == Ipv4Addr::new(255, 255, 255, 255) {
                    continue;
                }
                let ip = u32::from(v4.ip);
                let mask = u32::from(v4.netmask);
                targets.insert(Ipv4Addr::from(ip | !mask));
            }
        }
    }

    targets.into_iter().collect()
}

pub fn write_discover_request(socket: &UdpSocket, port: u16) -> io::Result<()> {
    let packet = vec![MessageType::DiscoverRequest as u8, PROTOCOL_VERSION];
    let mut sent_any = false;
    for target in discovery_broadcast_targets() {
        if socket.send_to(&packet, (target, port)).is_ok() {
            sent_any = true;
        }
    }
    if sent_any {
        Ok(())
    } else {
        Err(io::Error::new(
            ErrorKind::Other,
            "no se pudo enviar DISCOVER_REQUEST a ninguna interfaz de red",
        ))
    }
}

pub fn read_discover_packet(buf: &[u8], source: SocketAddr) -> io::Result<Option<DiscoverResponse>> {
    if buf.len() < 2 {
        return Ok(None);
    }
    let message_type = MessageType::from_u8(buf[0]).ok_or_else(|| {
        io::Error::new(ErrorKind::InvalidData, "unknown UDP message type")
    })?;
    if buf[1] != PROTOCOL_VERSION {
        return Ok(None);
    }
    match message_type {
        MessageType::DiscoverRequest => Ok(None),
        MessageType::DiscoverResponse => {
            let mut cursor = 2;
            let game_id = read_u16_from(buf, &mut cursor)?;
            let server_name = read_string_from(buf, &mut cursor)?;
            let tcp_port = read_u16_from(buf, &mut cursor)?;
            let state = GamePhase::from_u8(read_u8_from(buf, &mut cursor)?)
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "invalid game phase"))?;
            let player_count = read_u16_from(buf, &mut cursor)?;
            let maximum_players = read_u16_from(buf, &mut cursor)?;
            Ok(Some(DiscoverResponse {
                game_id,
                server_name,
                tcp_port,
                state,
                player_count,
                maximum_players,
                source,
            }))
        }
        _ => Ok(None),
    }
}

pub fn encode_discover_response(
    game_id: u16,
    server_name: &str,
    tcp_port: u16,
    state: GamePhase,
    player_count: u16,
    maximum_players: u16,
) -> Vec<u8> {
    let mut out = vec![MessageType::DiscoverResponse as u8, PROTOCOL_VERSION];
    write_u16(&mut out, game_id);
    write_string(&mut out, server_name);
    write_u16(&mut out, tcp_port);
    out.push(state as u8);
    write_u16(&mut out, player_count);
    write_u16(&mut out, maximum_players);
    out
}

pub fn write_framed(stream: &mut TcpStream, payload: &[u8]) -> io::Result<()> {
    let size = u16::try_from(payload.len())
        .map_err(|_| io::Error::new(ErrorKind::InvalidInput, "message too large"))?;
    stream.write_all(&size.to_be_bytes())?;
    stream.write_all(payload)?;
    Ok(())
}

pub fn read_framed(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut size_buf = [0u8; 2];
    stream.read_exact(&mut size_buf)?;
    let size = u16::from_be_bytes(size_buf) as usize;
    let mut payload = vec![0u8; size];
    stream.read_exact(&mut payload)?;
    Ok(payload)
}

pub fn decode_client_message(payload: &[u8]) -> io::Result<ClientMessage> {
    if payload.len() < 2 {
        return Err(io::Error::new(ErrorKind::InvalidData, "message too short"));
    }
    let message_type = MessageType::from_u8(payload[0])
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "unknown message type"))?;
    if payload[1] != PROTOCOL_VERSION {
        return Err(io::Error::new(ErrorKind::InvalidData, "unsupported protocol version"));
    }
    let mut cursor = 2;
    match message_type {
        MessageType::Join => Ok(ClientMessage::Join {
            name: read_string_from(payload, &mut cursor)?,
        }),
        MessageType::Input => {
            let player_id = read_u16_from(payload, &mut cursor)?;
            let direction = Direction::from_u8(read_u8_from(payload, &mut cursor)?)
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "invalid direction"))?;
            Ok(ClientMessage::Input { player_id, direction })
        }
        MessageType::Interact => Ok(ClientMessage::Interact {
            player_id: read_u16_from(payload, &mut cursor)?,
        }),
        MessageType::Leave => Ok(ClientMessage::Leave {
            player_id: read_u16_from(payload, &mut cursor)?,
        }),
        _ => Err(io::Error::new(
            ErrorKind::InvalidData,
            "unexpected message for client decoder",
        )),
    }
}

pub fn encode_client_message(message: &ClientMessage) -> Vec<u8> {
    let mut out = Vec::new();
    match message {
        ClientMessage::Join { name } => {
            out.push(MessageType::Join as u8);
            out.push(PROTOCOL_VERSION);
            write_string(&mut out, name);
        }
        ClientMessage::Input {
            player_id,
            direction,
        } => {
            out.push(MessageType::Input as u8);
            out.push(PROTOCOL_VERSION);
            write_u16(&mut out, *player_id);
            out.push(*direction as u8);
        }
        ClientMessage::Interact { player_id } => {
            out.push(MessageType::Interact as u8);
            out.push(PROTOCOL_VERSION);
            write_u16(&mut out, *player_id);
        }
        ClientMessage::Leave { player_id } => {
            out.push(MessageType::Leave as u8);
            out.push(PROTOCOL_VERSION);
            write_u16(&mut out, *player_id);
        }
    }
    out
}

pub fn encode_server_message(message: &ServerMessage) -> Vec<u8> {
    let mut out = Vec::new();
    match message {
        ServerMessage::JoinAccepted { player_id, game_id } => {
            out.push(MessageType::JoinAccepted as u8);
            out.push(PROTOCOL_VERSION);
            write_u16(&mut out, *player_id);
            write_u16(&mut out, *game_id);
        }
        ServerMessage::JoinRejected { reason } => {
            out.push(MessageType::JoinRejected as u8);
            out.push(PROTOCOL_VERSION);
            out.push(*reason as u8);
        }
        ServerMessage::LobbyState { state, players } => {
            out.push(MessageType::LobbyState as u8);
            out.push(PROTOCOL_VERSION);
            out.push(*state as u8);
            out.push(players.len() as u8);
            for player in players {
                write_u16(&mut out, player.player_id);
                write_string(&mut out, &player.name);
            }
        }
        ServerMessage::GameCountdown { seconds_remaining } => {
            out.push(MessageType::GameCountdown as u8);
            out.push(PROTOCOL_VERSION);
            out.push(*seconds_remaining);
        }
        ServerMessage::GameStarted {
            map_size,
            circle_radius,
            player_radius,
            player_speed,
            interaction_radius,
            tick_interval_ms,
            flag_status,
            flag_carrier_id,
            flag_x,
            flag_y,
            players,
        } => {
            out.push(MessageType::GameStarted as u8);
            out.push(PROTOCOL_VERSION);
            write_i32(&mut out, *map_size);
            write_i32(&mut out, *circle_radius);
            write_i32(&mut out, *player_radius);
            write_i32(&mut out, *player_speed);
            write_i32(&mut out, *interaction_radius);
            write_u16(&mut out, *tick_interval_ms);
            out.push(*flag_status as u8);
            write_u16(&mut out, *flag_carrier_id);
            write_i32(&mut out, *flag_x);
            write_i32(&mut out, *flag_y);
            out.push(players.len() as u8);
            for player in players {
                write_u16(&mut out, player.player_id);
                write_string(&mut out, &player.name);
                write_i32(&mut out, player.x);
                write_i32(&mut out, player.y);
                out.push(player.direction as u8);
                out.push(player.has_flag as u8);
            }
        }
        ServerMessage::GameState {
            tick,
            flag_status,
            flag_carrier_id,
            flag_x,
            flag_y,
            players,
        } => {
            out.push(MessageType::GameState as u8);
            out.push(PROTOCOL_VERSION);
            write_u32(&mut out, *tick);
            out.push(*flag_status as u8);
            write_u16(&mut out, *flag_carrier_id);
            write_i32(&mut out, *flag_x);
            write_i32(&mut out, *flag_y);
            out.push(players.len() as u8);
            for player in players {
                write_u16(&mut out, player.player_id);
                write_i32(&mut out, player.x);
                write_i32(&mut out, player.y);
                out.push(player.direction as u8);
                out.push(player.has_flag as u8);
            }
        }
        ServerMessage::FlagPickedUp { tick, player_id } => {
            out.push(MessageType::FlagPickedUp as u8);
            out.push(PROTOCOL_VERSION);
            write_u32(&mut out, *tick);
            write_u16(&mut out, *player_id);
        }
        ServerMessage::FlagStolen {
            tick,
            previous_carrier_id,
            new_carrier_id,
        } => {
            out.push(MessageType::FlagStolen as u8);
            out.push(PROTOCOL_VERSION);
            write_u32(&mut out, *tick);
            write_u16(&mut out, *previous_carrier_id);
            write_u16(&mut out, *new_carrier_id);
        }
        ServerMessage::PlayerDisconnected { player_id } => {
            out.push(MessageType::PlayerDisconnected as u8);
            out.push(PROTOCOL_VERSION);
            write_u16(&mut out, *player_id);
        }
        ServerMessage::GameOver {
            winner_id,
            winner_name,
            reason,
        } => {
            out.push(MessageType::GameOver as u8);
            out.push(PROTOCOL_VERSION);
            write_u16(&mut out, *winner_id);
            write_string(&mut out, winner_name);
            out.push(*reason as u8);
        }
        ServerMessage::Error { code, description } => {
            out.push(MessageType::Error as u8);
            out.push(PROTOCOL_VERSION);
            out.push(*code as u8);
            write_string(&mut out, description);
        }
    }
    out
}

pub fn decode_server_message(payload: &[u8]) -> io::Result<ServerMessage> {
    if payload.len() < 2 {
        return Err(io::Error::new(ErrorKind::InvalidData, "message too short"));
    }
    let message_type = MessageType::from_u8(payload[0])
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "unknown message type"))?;
    if payload[1] != PROTOCOL_VERSION {
        return Err(io::Error::new(ErrorKind::InvalidData, "unsupported protocol version"));
    }
    let mut cursor = 2;
    match message_type {
        MessageType::JoinAccepted => Ok(ServerMessage::JoinAccepted {
            player_id: read_u16_from(payload, &mut cursor)?,
            game_id: read_u16_from(payload, &mut cursor)?,
        }),
        MessageType::JoinRejected => {
            let reason = match read_u8_from(payload, &mut cursor)? {
                0x01 => JoinRejectReason::GameAlreadyStarted,
                0x02 => JoinRejectReason::GameFull,
                0x03 => JoinRejectReason::InvalidName,
                0x04 => JoinRejectReason::UnsupportedProtocolVersion,
                _ => {
                    return Err(io::Error::new(
                        ErrorKind::InvalidData,
                        "invalid join reject reason",
                    ))
                }
            };
            Ok(ServerMessage::JoinRejected { reason })
        }
        MessageType::LobbyState => {
            let state = GamePhase::from_u8(read_u8_from(payload, &mut cursor)?)
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "invalid lobby state"))?;
            let count = read_u8_from(payload, &mut cursor)? as usize;
            let mut players = Vec::with_capacity(count);
            for _ in 0..count {
                players.push(LobbyPlayer {
                    player_id: read_u16_from(payload, &mut cursor)?,
                    name: read_string_from(payload, &mut cursor)?,
                });
            }
            Ok(ServerMessage::LobbyState { state, players })
        }
        MessageType::GameCountdown => Ok(ServerMessage::GameCountdown {
            seconds_remaining: read_u8_from(payload, &mut cursor)?,
        }),
        MessageType::GameStarted => {
            let map_size = read_i32_from(payload, &mut cursor)?;
            let circle_radius = read_i32_from(payload, &mut cursor)?;
            let player_radius = read_i32_from(payload, &mut cursor)?;
            let player_speed = read_i32_from(payload, &mut cursor)?;
            let interaction_radius = read_i32_from(payload, &mut cursor)?;
            let tick_interval_ms = read_u16_from(payload, &mut cursor)?;
            let flag_status = FlagStatus::from_u8(read_u8_from(payload, &mut cursor)?)
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "invalid flag status"))?;
            let flag_carrier_id = read_u16_from(payload, &mut cursor)?;
            let flag_x = read_i32_from(payload, &mut cursor)?;
            let flag_y = read_i32_from(payload, &mut cursor)?;
            let count = read_u8_from(payload, &mut cursor)? as usize;
            let mut players = Vec::with_capacity(count);
            for _ in 0..count {
                players.push(PlayerWire {
                    player_id: read_u16_from(payload, &mut cursor)?,
                    name: read_string_from(payload, &mut cursor)?,
                    x: read_i32_from(payload, &mut cursor)?,
                    y: read_i32_from(payload, &mut cursor)?,
                    direction: Direction::from_u8(read_u8_from(payload, &mut cursor)?)
                        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "invalid direction"))?,
                    has_flag: read_u8_from(payload, &mut cursor)? != 0,
                });
            }
            Ok(ServerMessage::GameStarted {
                map_size,
                circle_radius,
                player_radius,
                player_speed,
                interaction_radius,
                tick_interval_ms,
                flag_status,
                flag_carrier_id,
                flag_x,
                flag_y,
                players,
            })
        }
        MessageType::GameState => {
            let tick = read_u32_from(payload, &mut cursor)?;
            let flag_status = FlagStatus::from_u8(read_u8_from(payload, &mut cursor)?)
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "invalid flag status"))?;
            let flag_carrier_id = read_u16_from(payload, &mut cursor)?;
            let flag_x = read_i32_from(payload, &mut cursor)?;
            let flag_y = read_i32_from(payload, &mut cursor)?;
            let count = read_u8_from(payload, &mut cursor)? as usize;
            let mut players = Vec::with_capacity(count);
            for _ in 0..count {
                players.push(PlayerStateWire {
                    player_id: read_u16_from(payload, &mut cursor)?,
                    x: read_i32_from(payload, &mut cursor)?,
                    y: read_i32_from(payload, &mut cursor)?,
                    direction: Direction::from_u8(read_u8_from(payload, &mut cursor)?)
                        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "invalid direction"))?,
                    has_flag: read_u8_from(payload, &mut cursor)? != 0,
                });
            }
            Ok(ServerMessage::GameState {
                tick,
                flag_status,
                flag_carrier_id,
                flag_x,
                flag_y,
                players,
            })
        }
        MessageType::FlagPickedUp => Ok(ServerMessage::FlagPickedUp {
            tick: read_u32_from(payload, &mut cursor)?,
            player_id: read_u16_from(payload, &mut cursor)?,
        }),
        MessageType::FlagStolen => Ok(ServerMessage::FlagStolen {
            tick: read_u32_from(payload, &mut cursor)?,
            previous_carrier_id: read_u16_from(payload, &mut cursor)?,
            new_carrier_id: read_u16_from(payload, &mut cursor)?,
        }),
        MessageType::PlayerDisconnected => Ok(ServerMessage::PlayerDisconnected {
            player_id: read_u16_from(payload, &mut cursor)?,
        }),
        MessageType::GameOver => Ok(ServerMessage::GameOver {
            winner_id: read_u16_from(payload, &mut cursor)?,
            winner_name: read_string_from(payload, &mut cursor)?,
            reason: GameOverReason::ExitedCircleWithFlag,
        }),
        MessageType::Error => {
            let code = match read_u8_from(payload, &mut cursor)? {
                0x01 => ErrorCode::InvalidMessage,
                0x02 => ErrorCode::InvalidEncoding,
                0x03 => ErrorCode::InvalidInput,
                0x04 => ErrorCode::UnknownPlayer,
                0x05 => ErrorCode::GameNotStarted,
                0x06 => ErrorCode::GameAlreadyStarted,
                0x07 => ErrorCode::GameFinished,
                0x08 => ErrorCode::UnsupportedProtocolVersion,
                _ => return Err(io::Error::new(ErrorKind::InvalidData, "invalid error code")),
            };
            Ok(ServerMessage::Error {
                code,
                description: read_string_from(payload, &mut cursor)?,
            })
        }
        _ => Err(io::Error::new(
            ErrorKind::InvalidData,
            "unexpected message for server decoder",
        )),
    }
}

fn read_u8_from(buf: &[u8], cursor: &mut usize) -> io::Result<u8> {
    if *cursor >= buf.len() {
        return Err(io::Error::new(ErrorKind::UnexpectedEof, "buffer underflow"));
    }
    let value = buf[*cursor];
    *cursor += 1;
    Ok(value)
}

fn read_u16_from(buf: &[u8], cursor: &mut usize) -> io::Result<u16> {
    if *cursor + 2 > buf.len() {
        return Err(io::Error::new(ErrorKind::UnexpectedEof, "buffer underflow"));
    }
    let value = u16::from_be_bytes([buf[*cursor], buf[*cursor + 1]]);
    *cursor += 2;
    Ok(value)
}

fn read_u32_from(buf: &[u8], cursor: &mut usize) -> io::Result<u32> {
    if *cursor + 4 > buf.len() {
        return Err(io::Error::new(ErrorKind::UnexpectedEof, "buffer underflow"));
    }
    let value = u32::from_be_bytes([
        buf[*cursor],
        buf[*cursor + 1],
        buf[*cursor + 2],
        buf[*cursor + 3],
    ]);
    *cursor += 4;
    Ok(value)
}

fn read_i32_from(buf: &[u8], cursor: &mut usize) -> io::Result<i32> {
    Ok(read_u32_from(buf, cursor)? as i32)
}

fn read_string_from(buf: &[u8], cursor: &mut usize) -> io::Result<String> {
    let len = read_u8_from(buf, cursor)? as usize;
    if *cursor + len > buf.len() {
        return Err(io::Error::new(ErrorKind::UnexpectedEof, "string overflow"));
    }
    let raw = &buf[*cursor..*cursor + len];
    *cursor += len;
    String::from_utf8(raw.to_vec())
        .map_err(|_| io::Error::new(ErrorKind::InvalidData, "invalid UTF-8 string"))
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn write_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    out.push(bytes.len().min(u8::MAX as usize) as u8);
    out.extend_from_slice(&bytes[..bytes.len().min(u8::MAX as usize)]);
}

#[cfg(test)]
mod discovery_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn targets_always_include_global_broadcast() {
        let targets = discovery_broadcast_targets();
        eprintln!("discovery targets detectados: {:?}", targets);
        assert!(targets.contains(&Ipv4Addr::new(255, 255, 255, 255)));
    }

    #[test]
    fn targets_include_at_least_one_interface_specific_broadcast() {
        let targets = discovery_broadcast_targets();
        let interface_specific = targets
            .iter()
            .filter(|addr| **addr != Ipv4Addr::new(255, 255, 255, 255))
            .count();
        assert!(
            interface_specific > 0,
            "se esperaba al menos una interfaz real ademas del broadcast global"
        );
    }

    #[test]
    fn write_discover_request_reaches_a_local_listener() {
        let listener = UdpSocket::bind(("0.0.0.0", 0)).expect("bind listener");
        listener.set_broadcast(true).expect("set_broadcast listener");
        listener
            .set_read_timeout(Some(Duration::from_millis(500)))
            .expect("set_read_timeout");
        let port = listener.local_addr().expect("local_addr").port();

        let sender = UdpSocket::bind(("0.0.0.0", 0)).expect("bind sender");
        sender.set_broadcast(true).expect("set_broadcast sender");
        write_discover_request(&sender, port).expect("write_discover_request");

        let mut buf = [0u8; 16];
        let (size, _source) = listener
            .recv_from(&mut buf)
            .expect("no llego ningun DISCOVER_REQUEST al listener local");
        assert_eq!(&buf[..size], &[MessageType::DiscoverRequest as u8, PROTOCOL_VERSION]);
    }

    #[test]
    fn discover_response_round_trips_custom_server_name() {
        let payload = encode_discover_response(1234, "Prueba de Su", 5000, GamePhase::Waiting, 1, 100);
        let source: SocketAddr = "127.0.0.1:9999".parse().unwrap();
        let response = read_discover_packet(&payload, source)
            .expect("decode ok")
            .expect("Some(response)");
        assert_eq!(response.server_name, "Prueba de Su");
    }
}
