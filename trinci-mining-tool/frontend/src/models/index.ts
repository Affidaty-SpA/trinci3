export interface INodeData {
    accountId: string;
    ip: string;
    connected?: boolean; // TODO: maybe usefull
    rest: number;
    socket: number,
    selected?: boolean;
    source?: string;
}
export interface NodeStats {
  bitbel: {time: number, units: number}[],
  tcoin: {time: number, units_in_stake: number, total: number}[],
  life_points: {time: number, units: number, total: number}[]
}

export interface INodeDB {
  health: HealthUpdate
  players: PlayersUpdate[],
  stats:NodeStats ,
  status: boolean
}
export interface OtherPlayer {
  account_id: string
  t_coin: number
  time: number
  block_height: number
}
export interface Health {
  last_time: number
  system_stats: SystemStats
  unconfirmed_pool_len: number
  block_height: number
}
export interface PlayerHandled {
  connection: Connection
  stats: NodeStats
  health: Health
  status: boolean
}
export interface Connection {
  ip: string
  rest: number
  socket: number
}
export interface IInitMiningTool {
  players: OtherPlayer[],
  players_handled: [string, PlayerHandled][]
}

export interface ISocketNodeUpdate {
    account_id: string
    health_update: HealthUpdate
    players_update: PlayersUpdate[]
    pop_update: PopUpdate
  }

  export interface HealthUpdate {
    block_height: number
    last_time: number
    system_stats: SystemStats
    unconfirmed_pool_len: number
  }

  export interface SystemStats {
    cpus_usage: CpusUsage
    disk_usage: DiskUsage
    mem_usage: MemUsage
    time?: number
  }

  export interface CpusUsage {
    measure: string
    total: number
    used: number
  }

  export interface DiskUsage {
    measure: string
    total: number
    used: number
  }

  export interface MemUsage {
    measure: string
    total: number
    used: number
  }

  export interface PlayersUpdate {
    account_id: string
    life_points: number
    units_in_stake: number
  }

  export interface PopUpdate {
    bitbel: Bitbel
    life_points: LifePoints
    tcoin: Tcoin
  }

  export interface Bitbel {
    time: number
    units: number
  }

  export interface LifePoints {
    time: number
    total: number
    units: number
  }

  export interface Tcoin {
    time: number
    total: number
    units_in_stake: number
  }

// {
//     "account_id": "QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2cvNL",
//     "health_update": {
//       "block_height": 615,
//       "last_time": 1711637156,
//       "system_stats": {
//         "cpus_usage": {
//           "measure": "Percent",
//           "total": 100,
//           "used": 94
//         },
//         "disk_usage": {
//           "measure": "Bytes",
//           "total": 2046659112960,
//           "used": 1632914755584
//         },
//         "mem_usage": {
//           "measure": "Bytes",
//           "total": 16315424768,
//           "used": 8429645824
//         }
//       },
//       "unconfirmed_pool_len": 0
//     },
//     "players_update": [
//       {
//         "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4qhP",
//         "life_points": 3,
//         "units_in_stake": 11300
//       },
//       {
//         "account_id": "QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2cvNL",
//         "life_points": 3,
//         "units_in_stake": 27101
//       }
//     ],
//     "pop_update": {
//       "bitbel": {
//         "time": 1711637142,
//         "units": 695000
//       },
//       "life_points": {
//         "time": 1711637142,
//         "total": 3,
//         "units": 3
//       },
//       "tcoin": {
//         "time": 1711637142,
//         "total": 27101,
//         "units_in_stake": 27101
//       }
//     }
//   }