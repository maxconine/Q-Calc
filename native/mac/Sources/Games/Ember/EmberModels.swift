import Foundation

// Ember & Frost's stage, state and events as the renderer reads them: the TS's own shapes (emberTypes.ts), decoded
// from the JSON the Rust core writes (EmberMatch.stageJson, EmberFrame.stateJson / eventsJson). Field names are the
// TS's, so EmberRenderer reads like emberDraw.ts.

enum Ember {
    static let TILE = 20.0
    static let VIEW_W = 640.0
    static let VIEW_H = 360.0
    static let PLAYER_W = 14.0
    static let PLAYER_H = 22.0
    static let POOL_SINK = 6.0
    static let ONEWAY_H = 6.0
    static let GRAVITY = 1400.0
    static let RUN_SPEED = 150.0
    static let STAGE_TICKS = 70.0
    static let CRUMBLE_TICKS = 36.0
    static let CRUMBLE_GONE_TICKS = 180.0
    static let ARROW_RESPAWN_TICKS = 150.0
    static let CANNON_FUSE_TICKS = 40.0
    static let CANNON_FLY_TICKS = 90.0
    static let GLOW_EMBER = 5.5 * 20.0
    static let GLOW_FROST = 3.0 * 20.0
    static let GLOW_SMALL = 1.5 * 20.0
    static let STEP_HZ = 60.0
    static let CH_LIT = "lights"

    static let T_EMPTY = 0
    static let T_WALL = 1
    static let T_LAVA = 2
    static let T_WATER = 3
    static let T_GOO = 4
    static let T_THIN = 5
    static let T_TRAMP = 6
    static let T_ONEWAY = 7
    static let T_CRUMBLE = 8
    static let T_LIGHT = 9
    static let T_DARK = 10

    static let IN_LEFT = 1
    static let IN_RIGHT = 2
    static let IN_UP = 4
    static let IN_DOWN = 8
}

struct ERect: Codable, Equatable {
    var x: Double
    var y: Double
    var w: Double
    var h: Double
}

struct EPoint: Codable, Equatable {
    var x: Double
    var y: Double
}

struct EStage: Decodable {
    struct Gem: Decodable { var x: Double; var y: Double; var el: String }
    struct Strip: Decodable { var x: Double; var y: Double; var w: Double; var h: Double; var ch: String; var ceil: Bool; var ticks: Double }
    struct Lever: Decodable { var x: Double; var y: Double; var w: Double; var h: Double; var ch: String; var on: Bool }
    struct Gate: Decodable { var x: Double; var y: Double; var w: Double; var h: Double; var open: [String]; var dir: String }
    struct Mover: Decodable { var x: Double; var y: Double; var w: Double; var h: Double; var to: EPoint; var when: [String]?; var speed: Double }
    struct Fan: Decodable { var x: Double; var y: Double; var w: Double; var h: Double; var when: [String]? }
    struct Emitter: Decodable { var x: Double; var y: Double; var dir: String; var when: [String]? }
    struct Mirror: Decodable { var x: Double; var y: Double; var slant: String }
    struct Sensor: Decodable { var x: Double; var y: Double; var w: Double; var h: Double; var ch: String }
    struct Portal: Decodable { var x: Double; var y: Double; var w: Double; var h: Double; var pair: String; var other: Int }
    struct Sign: Decodable { var x: Double; var y: Double; var text: String }
    struct Checkpoint: Decodable { var x: Double; var y: Double; var zone: ERect }
    struct Lamp: Decodable { var x: Double; var y: Double; var w: Double; var h: Double; var ticks: Double }
    struct Gswitch: Decodable { var x: Double; var y: Double; var w: Double; var h: Double; var who: String }
    struct Cannon: Decodable { var x: Double; var y: Double; var aim: String; var vx: Double; var vy: Double; var fire: [String]? }

    var index: Int
    var cols: Int
    var rows: Int
    var w: Double
    var h: Double
    var tiles: [Int]
    var dark: Bool
    var spawns: [EPoint]
    var doors: [ERect]
    var gems: [Gem]
    var plates: [Strip]
    var levers: [Lever]
    var buttons: [Strip]
    var gates: [Gate]
    var movers: [Mover]
    var boxes: [EPoint]
    var fans: [Fan]
    var ice: [ERect]
    var emitters: [Emitter]
    var mirrors: [Mirror]
    var sensors: [Sensor]
    var portals: [Portal]
    var signs: [Sign]
    var thin: [Int]
    var crumble: [Int]
    var checkpoints: [Checkpoint]
    var lamps: [Lamp]
    var gswitches: [Gswitch]
    var arrows: [EPoint]
    var keys: [EPoint]
    var locks: [ERect]
    var cannons: [Cannon]
    var darks: [ERect]
}

struct EPlayer: Decodable {
    var x: Double
    var y: Double
    var vx: Double
    var vy: Double
    var ground: Bool
    var face: Double
    var alive: Bool
    var inDoor: Bool
    var coyote: Double
    var buffer: Double
    var prevIn: Int
    var portalCd: Double
    var gems: Double
    var grav: Double
    var air: Double
    var cannon: Int
    var fly: Double
    var cannonCd: Double
}

struct EMoverState: Decodable { var x: Double; var y: Double; var t: Double; var dir: Double }
struct EBoxState: Decodable { var x: Double; var y: Double; var vx: Double; var vy: Double }

struct EState: Decodable {
    var tick: Double
    var phase: String
    var phaseT: Double
    var deaths: Double
    var stage: Int
    var players: [EPlayer]
    var taken: [Bool]
    var levers: [Bool]
    var buttons: [Double]
    var gates: [Double]
    var movers: [EMoverState]
    var boxes: [EBoxState]
    var ice: [Double]
    var thin: [Double]
    var mirrors: [String]
    var lit: Bool
    var litT: Double
    var arrows: [Double]
    var crumble: [Double]
    var keys: Double
    var keyTaken: [Bool]
    var locks: [Bool]
    var cannons: [Double]
    var cp: Int
    var on: [String]
    var plates: [Bool]?
    var sensors: [Bool]?
}

/// one event of any kind: only the fields its kind has are set
struct EEvent: Decodable {
    var k: String
    var p: Int?
    var tile: Int?
    var i: Int?
    var on: Bool?
    var x: Double?
    var y: Double?
    var cause: String?
    var v: Double?
    var grav: Double?
    var el: String?
    var stage: Int?
    var cp: Int?

    var ex: Double { x ?? 0 }
    var ey: Double { y ?? 0 }
}

enum EmberJSON {
    static let decoder = JSONDecoder()

    static func stage(_ text: String?) -> EStage? {
        guard let text, let data = text.data(using: .utf8) else { return nil }
        return try? decoder.decode(EStage.self, from: data)
    }

    static func state(_ text: String?) -> EState? {
        guard let text, let data = text.data(using: .utf8) else { return nil }
        return try? decoder.decode(EState.self, from: data)
    }

    static func events(_ text: String) -> [EEvent] {
        guard let data = text.data(using: .utf8) else { return [] }
        return (try? decoder.decode([EEvent].self, from: data)) ?? []
    }
}

/// what the renderer is handed each frame beside the states: the tiles as they behave now, beams, gate rects
struct EmberWorldNow {
    var tiles: [UInt8]
    var beams: [Double]
    var gates: [Double]

    /// tileAt: outside the stage is wall
    func tileAt(_ stage: EStage, _ tx: Int, _ ty: Int) -> Int {
        if tx < 0 || ty < 0 || tx >= stage.cols || ty >= stage.rows { return Ember.T_WALL }
        let i = ty * stage.cols + tx
        return i < tiles.count ? Int(tiles[i]) : stage.tiles[i]
    }

    func gateRect(_ i: Int) -> ERect {
        let k = i * 4
        guard k + 3 < gates.count else { return ERect(x: 0, y: 0, w: 0, h: 0) }
        return ERect(x: gates[k], y: gates[k + 1], w: gates[k + 2], h: gates[k + 3])
    }
}
