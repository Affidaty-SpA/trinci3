import { Manager, Socket } from "socket.io-client";
import { getUuidString } from "@/utils/utils";
import type { TrinciTokenAuth } from "@/router";
// import { useUserStore } from "@/stores/user.store";
// import { useTxStore, type IIndexerTx } from "@/stores/transaction.store";
import { useToast } from "vue-toastification";
import type { ISocketNodeUpdate, SystemStats } from "@/models";
import { useNodeStore } from "@/stores/node";

export interface SocketResponse {
  uuid: string;
  data: any;
  success: boolean;
}

export default class WSClient {
  //static socketManager: Manager;
  static socket: Socket;
  static roomMap = new Map<string, string>();
  static socketManager: Manager;
  static pendingUpdateBalance: boolean = false;
  static pendingSocketRequest = new Map<string, { resolve: Function; reject: Function }>();

  static async connect(token?: string): Promise<typeof WSClient> {
    return new Promise((resolve, reject) => {
      const socketServerProtocol  = "";//(document.location.protocol !== 'https:') ? 'ws://' : 'wss://';
      const fullPathSocket = `${socketServerProtocol}${document.location.hostname}:${import.meta.env.VITE_SOCKET_PORT}`;
      this.socketManager = new Manager('ws://127.0.0.1:7000', {
        // path:"/mining/ws",
        reconnectionDelayMax: 10000,
        //transports: ["websocket", "polling"]
        transports: ["websocket"]
      });
      this.socket = this.socketManager
        .socket("/", {
          auth: {
            token,
          },
        })
        .on("connect", () => {
          const { miningToolPerformance, allNodesData, setLastBlockHeight, setOtherPlayers } = useNodeStore();
          WSClient.socket.on('update:node', (ev: ISocketNodeUpdate) => {
            if (allNodesData.has(ev.account_id)) {
              const currentNode = allNodesData.get(ev.account_id)!;
              currentNode.health = ev.health_update;
              currentNode.stats.bitbel.push(ev.pop_update.bitbel)
              currentNode.stats.tcoin.push(ev.pop_update.tcoin)
              currentNode.stats.life_points.push(ev.pop_update.life_points)
              if (setLastBlockHeight(ev.health_update.block_height)) {
                setOtherPlayers(ev.players_update.map((p) => {
                  return {
                    account_id: p.account_id,
                    t_coin: p.units_in_stake,
                    time: ev.health_update.last_time,
                    block_height: ev.health_update.block_height
                  }
                }))
              }
            } else {
              // TODO: handle unknown node
            }
            console.log('ISocketNodeUpdate', ev);
          })
          WSClient.socket.on('update:mt', (ev: SystemStats) => {
            ev.time = new Date().getTime();
            miningToolPerformance.push(ev);
            console.log('SystemStats', ev);
          })
          // TODO: Add here realtime interactions
          // WSClient.socket.on('balance:update', (event: { account: string, amount: any, asset: string }) => {
          //   const { getBalancesAsObj, setBalance } = useUserStore()
          //   const tokens = Object.keys(getBalancesAsObj())
          //   if (tokens.includes(event.asset)) {
          //     setBalance(event.asset, event.amount.value[0])
          //   }
          // })
          // WSClient.socket.on('transactions:new', (event: IIndexerTx) => {
          //   const { transactions, appendNewTx } = useTxStore()
          //   appendNewTx(event);
          // })
          resolve(WSClient);
        })
    });
  }

  static disconnect() {
    WSClient.socket.disconnect();
    WSClient.roomMap.clear();
  }

  static async getSocket(token?: string): Promise<typeof WSClient> {
    const toast = useToast()
    return new Promise((resolve, reject) => {
      const storedToken = localStorage.getItem('trinci_token');
      if (!token && storedToken) {
        token = (JSON.parse(storedToken) as TrinciTokenAuth).token;
      }

      if (WSClient.socket && WSClient.socket.connected) return resolve(WSClient);
      WSClient.connect(token)
        .then((socketClient) => {

          if (socketClient.socket.connected) return resolve(socketClient);
          return reject("Error: Socket not connected!")
        })
        .catch(e => {
          toast.error(e)
          reject(e);
        })
    });
  }

  static joinRoom(room: string, accountId: string) { //TODO: La mappa sovrascrive sempre, ha senso il controllo ?
    if (!WSClient.roomMap.has(room)) {
      WSClient.roomMap.set(room, accountId);
      return WSClient.asyncRequest("v1.io.join", {
        rooms: [room],
        socketId: WSClient.socket.id,
      })
    } else {
      return Promise.resolve(true) //TODO: Se sono gia` nell stanza che cazzo fo ? potrei non esser su quella di moleculer
    }
  }

  static leaveRoom(room: string) {
    if (WSClient.roomMap.has(room)) {
      WSClient.roomMap.delete(room);
      this.socket.emit("call", "v1.io.leave", {
        rooms: [room],
        socketId: WSClient.socket.id,
      });
    }
  }

  static asyncRequest<T, S>(service: string, params: S) {
    return new Promise<T>((resolve, reject) => {
      const uuid = getUuidString();
      this.socket.once(uuid, (event: SocketResponse) => {
        event.success ? resolve(event.data) : reject(event.data);
      });
      this.socket.emit("call", service, { ...params, uuid });
    });
  }
}
