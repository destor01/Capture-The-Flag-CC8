use rand::Rng;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::protocol::{
    world_to_wire, Direction, FlagStatus, GameOverReason, GamePhase, LobbyPlayer, PlayerStateWire,
    PlayerWire, ServerMessage,
};

#[derive(Debug, Clone)]
pub struct Config {
    pub map_size: f32,
    pub circle_radius: f32,
    pub player_radius: f32,
    pub spawn_margin: f32,
    pub player_speed: f32,
    pub interaction_radius: f32,
    pub tick_interval_ms: u16,
    pub countdown_seconds: u8,
    pub maximum_players: u16,
    pub server_port: u16,
    pub discovery_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            map_size: 2000.0,
            circle_radius: 500.0,
            player_radius: 15.0,
            spawn_margin: 80.0,
            player_speed: 220.0,
            interaction_radius: 60.0,
            tick_interval_ms: 50,
            countdown_seconds: 5,
            maximum_players: 100,
            server_port: 5000,
            discovery_port: crate::protocol::DISCOVERY_PORT,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Player {
    pub player_id: u16,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub direction: Direction,
    pub has_flag: bool,
}

#[derive(Debug, Clone)]
pub struct Flag {
    pub status: FlagStatus,
    pub carrier_id: u16,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone)]
pub struct GameSession {
    pub game_id: u16,
    pub server_name: String,
    pub config: Config,
    pub state: GamePhase,
    pub tick: u32,
    pub players: BTreeMap<u16, Player>,
    pub next_player_id: u16,
    pub flag: Flag,
    pub winner_id: Option<u16>,
    pub pending_inputs: HashMap<u16, Direction>,
    pub pending_interacts: BTreeSet<u16>,
}

#[derive(Debug, Clone)]
pub enum TickEvent {
    FlagPickedUp { tick: u32, player_id: u16 },
    FlagStolen { tick: u32, previous_carrier_id: u16, new_carrier_id: u16 },
}

impl GameSession {
    pub fn new(server_name: String, config: Config) -> Self {
        Self {
            game_id: rand::thread_rng().gen_range(1000..=9999),
            server_name,
            config,
            state: GamePhase::Waiting,
            tick: 0,
            players: BTreeMap::new(),
            next_player_id: 1,
            flag: Flag {
                status: FlagStatus::Available,
                carrier_id: 0,
                x: 0.0,
                y: 0.0,
            },
            winner_id: None,
            pending_inputs: HashMap::new(),
            pending_interacts: BTreeSet::new(),
        }
    }

    pub fn lobby_players(&self) -> Vec<LobbyPlayer> {
        self.players
            .values()
            .map(|player| LobbyPlayer {
                player_id: player.player_id,
                name: player.name.clone(),
            })
            .collect()
    }

    pub fn add_player(&mut self, name: String) -> u16 {
        let player_id = self.next_player_id;
        self.next_player_id = self.next_player_id.saturating_add(1);
        self.players.insert(
            player_id,
            Player {
                player_id,
                name,
                x: 0.0,
                y: 0.0,
                direction: Direction::None,
                has_flag: false,
            },
        );
        player_id
    }

    pub fn remove_player(&mut self, player_id: u16) -> Option<Player> {
        self.pending_inputs.remove(&player_id);
        self.pending_interacts.remove(&player_id);
        let removed = self.players.remove(&player_id);
        if let Some(player) = &removed {
            if player.has_flag {
                self.flag.status = FlagStatus::Dropped;
                self.flag.carrier_id = 0;
                self.flag.x = player.x;
                self.flag.y = player.y;
            }
        }
        removed
    }

    pub fn queue_input(&mut self, player_id: u16, direction: Direction) {
        self.pending_inputs.insert(player_id, direction);
    }

    pub fn queue_interact(&mut self, player_id: u16) {
        self.pending_interacts.insert(player_id);
    }

    pub fn start_countdown(&mut self) -> bool {
        if self.state != GamePhase::Waiting || self.players.is_empty() {
            return false;
        }
        self.state = GamePhase::Starting;
        true
    }

    pub fn begin_running(&mut self) -> ServerMessage {
        self.state = GamePhase::Running;
        self.tick = 0;
        self.winner_id = None;
        let circle_radius = self.config.circle_radius;
        let spawn_margin = self.config.spawn_margin;
        self.flag = Flag {
            status: FlagStatus::Available,
            carrier_id: 0,
            x: 0.0,
            y: 0.0,
        };
        for player in self.players.values_mut() {
            let (x, y) = random_spawn(circle_radius, spawn_margin);
            player.x = x;
            player.y = y;
            player.direction = Direction::None;
            player.has_flag = false;
        }
        self.game_started_message()
    }

    pub fn reset_to_waiting(&mut self) {
        self.state = GamePhase::Waiting;
        self.tick = 0;
        self.winner_id = None;
        self.players.clear();
        self.pending_inputs.clear();
        self.pending_interacts.clear();
        self.flag = Flag {
            status: FlagStatus::Available,
            carrier_id: 0,
            x: 0.0,
            y: 0.0,
        };
    }

    pub fn game_started_message(&self) -> ServerMessage {
        ServerMessage::GameStarted {
            map_size: world_to_wire(self.config.map_size),
            circle_radius: world_to_wire(self.config.circle_radius),
            player_radius: world_to_wire(self.config.player_radius),
            player_speed: world_to_wire(self.config.player_speed),
            interaction_radius: world_to_wire(self.config.interaction_radius),
            tick_interval_ms: self.config.tick_interval_ms,
            flag_status: self.flag.status,
            flag_carrier_id: self.flag.carrier_id,
            flag_x: world_to_wire(self.flag.x),
            flag_y: world_to_wire(self.flag.y),
            players: self
                .players
                .values()
                .map(|player| PlayerWire {
                    player_id: player.player_id,
                    name: player.name.clone(),
                    x: world_to_wire(player.x),
                    y: world_to_wire(player.y),
                    direction: player.direction,
                    has_flag: player.has_flag,
                })
                .collect(),
        }
    }

    pub fn game_state_message(&self) -> ServerMessage {
        ServerMessage::GameState {
            tick: self.tick,
            flag_status: self.flag.status,
            flag_carrier_id: self.flag.carrier_id,
            flag_x: world_to_wire(self.flag.x),
            flag_y: world_to_wire(self.flag.y),
            players: self
                .players
                .values()
                .map(|player| PlayerStateWire {
                    player_id: player.player_id,
                    x: world_to_wire(player.x),
                    y: world_to_wire(player.y),
                    direction: player.direction,
                    has_flag: player.has_flag,
                })
                .collect(),
        }
    }

    pub fn advance_tick(&mut self) -> (Vec<TickEvent>, ServerMessage, Option<ServerMessage>) {
        self.tick += 1;
        for (player_id, direction) in self.pending_inputs.drain() {
            if let Some(player) = self.players.get_mut(&player_id) {
                player.direction = direction;
            }
        }

        let step = self.config.player_speed * self.config.tick_interval_ms as f32 / 1000.0;
        let half_map = self.config.map_size / 2.0;
        for player in self.players.values_mut() {
            match player.direction {
                Direction::None => {}
                Direction::Up => player.y -= step,
                Direction::Down => player.y += step,
                Direction::Left => player.x -= step,
                Direction::Right => player.x += step,
            }
            player.x = player.x.clamp(-half_map, half_map);
            player.y = player.y.clamp(-half_map, half_map);
        }

        if self.flag.status == FlagStatus::Carried {
            if let Some(carrier) = self.players.get(&self.flag.carrier_id) {
                self.flag.x = carrier.x;
                self.flag.y = carrier.y;
            }
        }

        let snapshot = self.players.clone();
        let snapshot_flag = self.flag.clone();
        let mut events = Vec::new();
        let pending_interacts: Vec<u16> = self.pending_interacts.iter().copied().collect();
        for player_id in pending_interacts {
            let Some(actor) = snapshot.get(&player_id) else {
                continue;
            };
            if actor.has_flag {
                continue;
            }

            match snapshot_flag.status {
                FlagStatus::Available | FlagStatus::Dropped => {
                    if distance(actor.x, actor.y, snapshot_flag.x, snapshot_flag.y)
                        <= self.config.interaction_radius
                        && self.flag.carrier_id == 0
                    {
                        self.give_flag(player_id);
                        events.push(TickEvent::FlagPickedUp {
                            tick: self.tick,
                            player_id,
                        });
                    }
                }
                FlagStatus::Carried => {
                    let carrier_id = snapshot_flag.carrier_id;
                    if carrier_id != 0 && carrier_id != player_id {
                        if let Some(target) = snapshot.get(&carrier_id) {
                            if distance(actor.x, actor.y, target.x, target.y)
                                <= self.config.interaction_radius
                                && self.flag.carrier_id == carrier_id
                            {
                                self.transfer_flag(carrier_id, player_id);
                                events.push(TickEvent::FlagStolen {
                                    tick: self.tick,
                                    previous_carrier_id: carrier_id,
                                    new_carrier_id: player_id,
                                });
                            }
                        }
                    }
                }
                FlagStatus::Outside => {}
            }
        }
        self.pending_interacts.clear();

        if self.flag.status == FlagStatus::Carried {
            if let Some(carrier) = self.players.get(&self.flag.carrier_id) {
                self.flag.x = carrier.x;
                self.flag.y = carrier.y;
                let distance_from_center = distance(carrier.x, carrier.y, 0.0, 0.0);
                if distance_from_center - self.config.player_radius > self.config.circle_radius {
                    self.state = GamePhase::Finished;
                    self.flag.status = FlagStatus::Outside;
                    self.winner_id = Some(carrier.player_id);
                    let final_state = self.game_state_message();
                    let game_over = ServerMessage::GameOver {
                        winner_id: carrier.player_id,
                        winner_name: carrier.name.clone(),
                        reason: GameOverReason::ExitedCircleWithFlag,
                    };
                    return (events, final_state, Some(game_over));
                }
            }
        }

        (events, self.game_state_message(), None)
    }
    fn give_flag(&mut self, player_id: u16) {
        for player in self.players.values_mut() {
            player.has_flag = player.player_id == player_id;
        }
        self.flag.status = FlagStatus::Carried;
        self.flag.carrier_id = player_id;
        if let Some(player) = self.players.get(&player_id) {
            self.flag.x = player.x;
            self.flag.y = player.y;
        }
    }

    fn transfer_flag(&mut self, previous_carrier_id: u16, new_carrier_id: u16) {
        if let Some(previous) = self.players.get_mut(&previous_carrier_id) {
            previous.has_flag = false;
        }
        if let Some(new_carrier) = self.players.get_mut(&new_carrier_id) {
            new_carrier.has_flag = true;
            self.flag.status = FlagStatus::Carried;
            self.flag.carrier_id = new_carrier_id;
            self.flag.x = new_carrier.x;
            self.flag.y = new_carrier.y;
        }
    }
}

fn random_spawn(circle_radius: f32, spawn_margin: f32) -> (f32, f32) {
    let mut rng = rand::thread_rng();
    let angle = rng.gen_range(0.0..std::f32::consts::TAU);
    let radius = circle_radius + spawn_margin;
    (radius * angle.cos(), radius * angle.sin())
}

fn distance(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
}
