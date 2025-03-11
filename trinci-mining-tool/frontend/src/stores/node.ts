import { http } from '@/interceptor/http.interceptor'
import type { IInitMiningTool, INodeDB, INodeData, ISocketNodeUpdate, PlayerHandled, OtherPlayer, SystemStats } from '@/models'
import { defineStore } from 'pinia'
import { useRootStore } from '@/stores/index.store'
import { lazyPush } from '@/utils/utils'
import WSClient from '@/utils/wsClient'

export interface IGenericNodesData {
  miningStatus: boolean;
  activeNodeCount: number;
  disabledNodeCount: number;
  playerCount: number;
  npcCount: number;
  totalPlayers: number;
  totalBitbel: number;
  tCoinInStake: number;
  totalTCoin: number;
}
export interface INodeStore {
  nodesData: INodeData[];
  errorMsg: string | undefined;
  socketNodeData: Map<string, ISocketNodeUpdate>;
  allNodesData: Map<string, PlayerHandled>;
  activeNodes: [string, boolean][];
  activeMiningNodes: [string, boolean][];
  lastBlockHeight: number;
  genericNodesData: IGenericNodesData;
  nodeData?: PlayerHandled;
  otherNodes: OtherPlayer[];
  currentAccountId: string;
  miningToolPerformance: SystemStats[]
}

const initNodeStore: INodeStore = {
  errorMsg: undefined,
  nodesData: [],
  socketNodeData: new Map(),
  allNodesData: new Map(),
  activeNodes: [],
  activeMiningNodes: [],
  lastBlockHeight: 0,
  genericNodesData: {} as IGenericNodesData,
  nodeData: {} as PlayerHandled,
  otherNodes: [],
  currentAccountId: "",
  miningToolPerformance: []
}

export const useNodeStore = defineStore({
  id: 'nodeStore',
  state: () => ({
    ...initNodeStore
  }),
  getters: {
    nodeByAccountId: (state) => {
      return (accountId: string) => state.socketNodeData.get(accountId);
    },
    isAlive: (state) => {
      return (time: number) => {
        return true; // TODO: handle isAlive fn
      }
    }
  },
  actions: {
    async scanNetwork(next: () => void = () => {}) {
    //   return new Promise((resolve) => {
    //     setTimeout(() => {
    //       const mockedNodes: INodeData[] = Array(6)
    //         .fill(null)
    //         .map(() => {
    //           return {
    //             connected: false,
    //             ip: '10.0.0.' + Math.round(Math.random() * 254),
    //             rest: 9001,
    //             socket: 9002,
    //             accountId: 'accountId' + Math.random(),
    //             selected: false,
    //             source: "scan"
    //           }
    //         })
    //       next()
    //       lazyPush(200, this.nodesData, mockedNodes).then(resolve);
    //     }, 5000)
    //   })
      return http<INodeData[]>({
        method: 'get',
        url: '/mining/scan'
      }).then((res) => {
          return new Promise((resolve, reject) => {
            // console.log('NODES DATA', res)
            if (res.length > 0) {
              setTimeout(() => {next()}, 1000)
              lazyPush(
                200,
                this.nodesData,
                res.map((item) => {
                  item.selected = false
                  item.source = 'scan'
                  return item;
                })
              ).then(() => {
                resolve(res);
              })
            } else {
              reject('No nodes found');
            }
          })
        }).catch(console.error)
    },
    async addNodeToNtw(payload: {ip: string, rest: number}) {
        console.log('addNodeToNtw', payload)
        // this.nodesData.push({
        //     ...payload,
        //     socket: 2000,
        //     accountId: "manual-node",
        //     source: "manual",
        //     selected: true
        // })
        // return;
      // backend validation
      const { setShowLoader } = useRootStore()
      setShowLoader(true)
      return http<any>({
        method: 'post',
        url: '/mining/manual_add_node',
        data: {
            ip: payload.ip,
            rest: +payload.rest
        }
      }).then((res) => {
            console.log('ADD NODE RES ==> ', res)
            if (!this.nodesData.find((node) => node.accountId === res.accountId)) {
                this.nodesData.push({
                    ...res,
                    source: "manual",
                    selected: true
                })
            } else {
                this.errorMsg = "This node has been already discovered"
            }
        }).catch((err) => {
          this.errorMsg = err;
          console.error(err);
        }).finally(() => setShowLoader(false));
    },
    setErrorMsg(payload: string) {
      this.errorMsg = payload
    },
    updateSocketNodeData(payload: ISocketNodeUpdate) {
      if (this.socketNodeData.has(payload.account_id)) {
        const currNode = this.socketNodeData.get(payload.account_id)!;
        if (payload.health_update.block_height >= currNode.health_update.block_height ) {
          this.socketNodeData.set(payload.account_id, payload);
        }
      } else {
        this.socketNodeData.set(payload.account_id, payload);
      }
    },
    setActiveNodes(payload: [string, boolean][]) {
      this.activeNodes = payload;
    },
    setOtherPlayers(payload: OtherPlayer[]) {
      this.otherNodes = payload;
    },
    setActiveMiningNodes(payload: [string, boolean][]) {
      this.activeMiningNodes = payload;
    },
    setLastBlockHeight(payload: number): boolean {
      if (this.lastBlockHeight <= payload) {
        this.lastBlockHeight = payload;
        return true;
      }
      return false;
    },
    setGenericNodesData(payload: IGenericNodesData) {
      this.genericNodesData = payload;
    },
    setCurrentNode(accountId: string) {
      this.nodeData = this.allNodesData.get(accountId);
      this.currentAccountId = accountId;
      // console.log('SET', this.nodeData)
      return this.nodeData;
    },
    updateNodeStatus(newStatus: boolean, accountId: string) {
      this.nodeData = this.allNodesData.get(accountId);
      this.nodeData!.status = newStatus;
    },
    async initMiningTool(autoConnect: boolean = true) {
      const res = {
        "players": [
          {
            "account_id": "123xyz",
            "block_height": 42,
            "t_coin": 100,
            "time": 1712133509
          },
          {
            "account_id": "345qwe",
            "block_height": 42,
            "t_coin": 2000,
            "time": 1712133509
          },
          {
            "account_id": "678asd",
            "block_height": 42,
            "t_coin": 421,
            "time": 1712133509
          }
        ],
        "players_handled": [
          [
            "678asd",
            {
              "connection": {
                "ip": "10.0.0.32",
                "rest": 9001,
                "socket": 9002
              },
              "health": {
                "block_height": 421,
                "last_time": 1712133609,
                "system_stats": {
                  "cpus_usage": {
                    "measure": "percent",
                    "total": 1000,
                    "used": 1000
                  },
                  "disk_usage": {
                    "measure": "Bytes",
                    "total": 1024,
                    "used": 500
                  },
                  "mem_usage": {
                    "measure": "Bytes",
                    "total": 2048,
                    "used": 347
                  }
                },
                "unconfirmed_pool_len": 100
              },
              "stats": {
                "bitbel": [
                  {
                    "time": 1712133409,
                    "units": 4000000
                  },
                  {
                    "time": 1712133509,
                    "units": 2002000
                  }
                ],
                "life_points": [
                  {
                    "time": 1712133409,
                    "total": 3,
                    "units": 3
                  },
                  {
                    "time": 1712133509,
                    "total": 3,
                    "units": 3
                  }
                ],
                "tcoin": [
                  {
                    "time": 1712133409,
                    "total": 421,
                    "units_in_stake": 100
                  },
                  {
                    "time": 1712133509,
                    "total": 421,
                    "units_in_stake": 421
                  }
                ]
              },
              "status": true
            }
          ]
        ]
      } as IInitMiningTool;// unused mocked data

      const emptyres = {
        "players": [],
        "players_handled": [
          [
            "678asd",
            {
              "connection": {
                "ip": "10.0.0.32",
                "rest": 9001,
                "socket": 9002
              },
              "health": {
                "block_height": 0,
                "last_time": 0,
                "system_stats": {
                  "cpus_usage": {
                    "measure": "percent",
                    "total": 0,
                    "used": 0
                  },
                  "disk_usage": {
                    "measure": "Bytes",
                    "total": 0,
                    "used": 0
                  },
                  "mem_usage": {
                    "measure": "Bytes",
                    "total": 0,
                    "used": 0
                  }
                },
                "unconfirmed_pool_len": 0
              },
              "stats": {
                "bitbel": [
                  {
                    "time": 0,
                    "units": 0
                  },
                  {
                    "time": 0,
                    "units": 0
                  }
                ],
                "life_points": [
                  {
                    "time": 0,
                    "total": 0,
                    "units": 0
                  },
                  {
                    "time": 0,
                    "total": 0,
                    "units": 0
                  }
                ],
                "tcoin": [
                  {
                    "time": 0,
                    "total": 0,
                    "units_in_stake": 0
                  },
                  {
                    "time": 0,
                    "total": 0,
                    "units_in_stake": 0
                  }
                ]
              },
              "status": true
            }
          ]
        ]
      } as IInitMiningTool;// unused mocked data
      return http<IInitMiningTool>({
        method: 'get',
        url: '/mining/refresh'
      }).then((res) => {
        res.players_handled.forEach(([accountId, data]) => {
          // TODO: fix Bitbel units
          data.stats.bitbel = data.stats.bitbel.map(item => {
            item.units = +(item.units / 1000).toFixed(0);
            return item;
          })
          // handle last block height
          this.setLastBlockHeight(data.health.block_height);
          this.allNodesData.set(accountId, data);
        });
        this.otherNodes = res.players
        return res;
      }).then((res) => {
        // open connection with socket
        if(autoConnect) {
          WSClient.getSocket().then((client) => {
            // console.log('CONNECTED?', client.socket.connected)
          })
        }
        return res;
      })
    }
  }
})
