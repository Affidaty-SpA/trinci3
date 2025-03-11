import t2libcore from "@affidaty/t2-lib-core";
import { http } from "@/interceptor/http.interceptor";

export class EphemeralTx {
    static network = "t3MiningTool"
    target: string;
    method: string;
    args: {};
    tx: t2libcore.UnitaryTransaction;
    constructor(target: string, method: string, args: object | undefined) {
        this.target = target;
        this.method = method;
        this.args = args as {};
        this.tx = this.prepareUnitTx();
    }
    prepareUnitTx() {
        const tx = new t2libcore.UnitaryTransaction();
        tx.genNonce();
        tx.data.networkName = EphemeralTx.network;
        tx.data.accountId = this.target;
        tx.data.maxFuel = 0;
        tx.data.smartContractMethod = this.method;
        tx.data.smartContractHashHex = "";
        if (Object.keys(this.args).length > 0) {
            tx.data.smartContractMethodArgs = this.args;
        }
        return tx;
    }
    async sign() {
        return (window as any).AuthIn4rya.signTransaction(this.tx).then((signedTX: Object) => {
            return t2libcore.base58Encode(t2libcore.Utils.objectToBytes(signedTX));
        });
    }
    async signAndSubmit() {
        return (window as any).AuthIn4rya.signTransaction(this.tx).then((signedTX: t2libcore.IUnitaryTxObject) => {
            return this.tx.fromObject(signedTX).then(() => {
                return this.tx.toBytes();
            });
        });
    }

    async initRemoteProcedureCall(): Promise<[string, boolean][]> {
        return new Promise((resolve, reject) => {
            this.signAndSubmit().then((bytesTx: Uint8Array) => {
                http<[string, boolean][]>({
                    method: "post",
                    url: "/mining/rpc",
                    data: [...bytesTx]
                }).then(resolve).catch(reject);
            }).catch(reject);
        })
    }
    async startAndStopMining() {
        return new Promise((resolve, reject) => {
            this.signAndSubmit().then((bytesTx: Uint8Array) => {
                http<[string, boolean][]>({
                    method: "post",
                    url: "/mining/rpc",
                    data: [...bytesTx]
                }).then(resolve).catch(reject);
            }).catch(reject);
        })
    }
}