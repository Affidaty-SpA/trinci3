<script setup lang="ts">
"use strict";
// This starter template is using Vue 3 <script setup> SFCs
import { invoke } from "@tauri-apps/api/tauri";

import { listen } from "@tauri-apps/api/event";

import { onMounted, ref } from "vue";
// @ts-ignore
import AOS from "aos";
import "aos/dist/aos.css";
const BuiltBlocks = ref<number>(0); // number of blocks built
const AvgBuiltBlocks = ref<number>(0); // average number of blocks built
const ExecutedTxs = ref<number>(0); // number of executed Txs
const AvgExecutedTxs = ref<number>(0); // average number of executed Txs
const ExchangedMsgs = ref<number>(0); // number of exchanged messages
const listenRust = ref(true);
interface INode {
	id: string;
	hashColor: string; // Derived from DB hash.
	delay: number;
	borderColor: string; // Derived from status (offline|online).
	isLeader: boolean;
	blocks_height: number;
	online: boolean;
}

interface IReloadedNode {
	node_id: string;
	connected: boolean;
	db_hash: number;
	height: number;
	alignments_counter: number;
	leader: string;
}
interface INodeEvent {
	node_id: string;
	event_type: string;
	data: any;
}

let previous_leader_id = ref<string>("");
let test_leader_offline = ref<boolean>(false);
const new_bootstrap_id = ref<string>("");
const the_logger = ref<any>("");
const new_nodes_integer = ref<number>(10);
const new_players_integer = ref<number>(4);
const playground_ready = ref<boolean>(false);
const tx_node_id = ref<string>("");
const new_leader = ref<string>("");
const node_ready = ref<boolean>(true);

const changingLeader = ref<boolean>(false);

let node_info = ref<any>("");
const show_info = ref<boolean>(false);

let eventArray = ref<INodeEvent[]>([]);
let test_attempt = ref<number>(0);
const startPlayground = async () => {
	if (!playground_ready.value) {
		let nodes_number = new_nodes_integer.value || 10;
		let players_number = new_players_integer.value || 4;
		invoke("start_panopticon", {
			nodesNumber: nodes_number,
			playersNumber: players_number,
		}).then(() => {
			playground_ready.value = true;
		});
	}
};

const addNewNode = async () => {
	if (playground_ready.value) {
		if (new_bootstrap_id.value == "") {
			new_bootstrap_id.value =
				nodes.value[Math.floor(Math.random() * nodes.value.length)].id;
		}
		invoke("add_node", { bootstrapNodeId: new_bootstrap_id.value });
		new_bootstrap_id.value =
			nodes.value[Math.floor(Math.random() * nodes.value.length)].id;
	}
};

const send_tx = async () => {
	if (tx_node_id.value == "") {
		tx_node_id.value =
			nodes.value[Math.floor(Math.random() * nodes.value.length)].id;
	}
	invoke("send_tx", {
		nodeId: tx_node_id.value,
	});
	tx_node_id.value =
		nodes.value[Math.floor(Math.random() * nodes.value.length)].id;
};

const set_node_status = async (node: INode) => {
	return invoke<boolean>("switch_node", {
		nodeId: node.id,
	});
};

const get_neighbours = (node: INode) => {
	return invoke<string[]>("get_neighbors", { nodeId: node.id });
};

const get_node_info = (nodeId: string) => {
	return invoke("get_node_info", { nodeId });
};

const showNodeInfo = (nodeId: string) => {
	get_node_info(nodeId).then((info) => {
		show_info.value = true;
		setTimeout(() => {
			show_info.value = false;
		}, 5000);
		node_info = JSON.stringify(info, null, 2);
	});
};

const stringToColour = (str: string): string => {
	let hash = 0;
	for (let i = 0; i < str.length; i++) {
		hash = str.charCodeAt(i) + ((hash << 5) - hash);
	}
	let colour = "#";
	for (let i = 0; i < 3; i++) {
		const value = (hash >> (i * 8)) & 0xff;
		colour += ("00" + value.toString(16)).substr(-2);
	}
	return colour;
};

const switch_listening = () => {
	listenRust.value = !listenRust.value;

	return invoke("switch_listening");
};

const theLogger = (tx: any) => {
	the_logger.value = tx;
};

const hardTest = async () => {
	const nodeIds = nodes.value.map((node) => node.id);

	invoke("test", {
		nodes: nodeIds,
		txNumber: 15000,
	});
};

const testLeaderOffline = async () => {
	test_leader_offline.value = !test_leader_offline.value;
};

const easyTest = () => {
	const txSendingInterval = setInterval(() => {
		invoke("send_tx", {
			nodeId: nodes.value[Math.floor(Math.random() * nodes.value.length)].id,
		});
	}, 1000); // Send tx every 1 second

	setTimeout(() => {
		clearInterval(txSendingInterval); // Stop sending txs after a certain time
	}, 30000); // Stop after 30 seconds
};

const midTest = () => {
	const txSendingInterval = setInterval(() => {
		invoke("send_tx", {
			nodeId: nodes.value[Math.floor(Math.random() * nodes.value.length)].id,
		});
	}, 1); // Send tx every 1 millisecond

	setTimeout(() => {
		clearInterval(txSendingInterval); // Stop sending txs after a certain time
	}, 30000); // Stop after 30 seconds
};

const nodes = ref<INode[]>([]);

// Check out https://vuejs.org/api/sfc-script-setup.html#script-setup
onMounted(async () => {
	window.onerror = (event: string | Event, source?: string | undefined) => {
		theLogger({ event, source });
	};
	AOS.init({
		easing: "ease-in-out-back",
		duration: 1000,
	});

	invoke<IReloadedNode[]>("reload_nodes").then((reloaded_nodes) => {
		nodes.value = reloaded_nodes.map((n) => {
			new_leader.value = n.leader;
			return {
				id: n.node_id,
				hashColor: stringToColour(n.db_hash + "-1234"),
				borderColor: n.connected ? "white" : "red",
				isLeader: n.leader == n.node_id,
				delay: 600,
				blocks_height: n.height,
				online: n.connected,
			};
		});

		playground_ready.value = nodes.value.length > 0;
		if (!listenRust.value) {
			invoke("switch_listening");
			listenRust.value = true;
		}
	});

	await listen("rust_event", (event: any) => {
		//console.log("Started the Listen event from rust", event);

		const nodeEvent = JSON.parse(event.payload) as INodeEvent;

		if (nodeEvent.event_type === "PlaygroundStatsRefresh") {
			BuiltBlocks.value = nodeEvent.data.blocks_executed_count;
			AvgBuiltBlocks.value = nodeEvent.data.sampling.avg_blocks_in_sec;
			ExecutedTxs.value = nodeEvent.data.txs_executed_count;
			AvgExecutedTxs.value = nodeEvent.data.sampling.avg_txs_in_sec;
			ExchangedMsgs.value = nodeEvent.data.exchanged_msgs_count;
		}

		if (!listenRust.value) {
			return;
		}
		theLogger(nodeEvent);
		// event.event is the event name (useful if you want to use a single callback fn for multiple event types)
		// event.payload is the payload object

		//console.log(JSON.parse(event.payload).event_type);

		//eventArray.value.push(nodeEvent);
		// change border color
		nodes.value.find((node) => {
			if (node.id == nodeEvent.node_id && node.online) {
				const hashColor =
					nodeEvent.event_type + JSON.stringify(nodeEvent.data);
				node.borderColor = stringToColour(hashColor);
				setTimeout(() => {
					node.borderColor = "white";
				}, 1);
			}
		});

		// new node added in playground!
		if (nodeEvent.event_type === "NewNodeRunning") {
			node_ready.value = true;
			nodes.value.push({
				id: nodeEvent.node_id,
				hashColor: stringToColour(nodeEvent.data.hash + "-1234"),
				borderColor: "white",
				isLeader: nodeEvent.data.leader == nodeEvent.node_id,
				delay: 600,
				blocks_height: 0,
				online: true,
			});
		}

		if (nodeEvent.event_type === "BlockExecuted") {
			nodes.value.find((node) => {
				if (node.id == nodeEvent.node_id) {
					node.hashColor = stringToColour(nodeEvent.data.hash + "");
					node.blocks_height = nodeEvent.data.height;
				}
			});
		}

		if (nodeEvent.event_type == "NodeConnected") {
			//console.log("ChangeStatus of node", nodeEvent);
			nodes.value = nodes.value.map((node) => {
				if (node.id === nodeEvent.node_id) {
					node.online = true;
				}
				return node;
			});
		}

		if (nodeEvent.event_type === "NodeChangedLeader") {
			nodes.value = nodes.value.map((node) => {
				node.isLeader = node.id === nodeEvent.data.leader_id ? true : false;

				if (test_leader_offline.value) {
					if (node.id === nodeEvent.data.leader_id) {
						node.online = false;

						// Connecting again previous validator
						invoke<boolean>("switch_node", {
							nodeId: previous_leader_id.value,
						});

						// Disconnecting new validator
						invoke<boolean>("switch_node", {
							nodeId: node.id,
						});

						previous_leader_id.value = node.id;
					} else {
						node.online = true;
					}
				}

				return node;
			});
		}
	});
});
</script>

<template>
	<div class="box">
		<div class="stats" v-if="playground_ready">
			Built Blocks:
			{{ BuiltBlocks }} <br />
			Built Blocks per second:
			{{ AvgBuiltBlocks }} <br />
			Executed Txs:
			{{ ExecutedTxs }} <br />
			Executed Txs per second:
			{{ AvgExecutedTxs }} <br />
			Exchanged Messages:
			{{ ExchangedMsgs }}
		</div>
		<div class="container">
			<div class="nodes">
				<div class="exagon-container">
					<div data-aos="zoom-in" data-aos-offset="-500" data-aos-duration="100" v-for="node of nodes"
						v-bind:key="node.id" :data-aos-delay="node.delay"
						:style="{ 'background-color': `${node.borderColor}` }">
						<span title="Double click to set online/offline this node" @click="showNodeInfo(node.id)"
							@dblclick="() => {
				set_node_status(node).then(
					(status) => (node.online = status)
				);
			}
			" class="node" :style="{ 'background-color': `${node.hashColor}` }" @mouseover="() => {
				get_neighbours(node).then((neighbours) =>
					nodes
						.filter((n) => neighbours.includes(n.id))
						.map((i) => (i.borderColor = 'yellow'))
				);
			}
			" @mouseout="() => {
				get_neighbours(node).then((neighbours) =>
					nodes
						.filter((n) => neighbours.includes(n.id))
						.map((n) => (n.borderColor = 'white'))
				);
			}
			">
							<p :class="{
			nodeIsLeader: node.isLeader,
			nodeIsOffline: !node.online,
		}">
								{{ node.id }}<br />
								H:{{ node.blocks_height }}
							</p>
						</span>
					</div>
				</div>
			</div>
			<div v-if="show_info" class="node_info">Node Info:{{ node_info }}</div>
			<span class="ready">Playground ready : {{ playground_ready }}</span>
		</div>
		<div class="footer">
			<div v-if="!playground_ready">
				Number of nodes:
				<input type="number" v-model="new_nodes_integer" name="new_nodes_integer"
					placeholder="*number_of_nodes" />
				Number of players:
				<input type="number" v-model="new_players_integer" name="new_players_integer"
					placeholder="*number_of_players" />
				<button @click="() => startPlayground()">Start Playground</button>

				<hr />
			</div>
			<div v-if="playground_ready">
				<!--  v-if="node_ready" -->
				bootstrap id to connect to:
				<input type="text" v-model="new_bootstrap_id" name="new_bootstrap_id"
					placeholder="*add a new_bootstrap_id" />
				<button @click="() => addNewNode()">add</button>
				<button @click="switch_listening()">
					{{ listenRust ? "Stop" : "Start" }}
					Catch events
				</button>
				<hr />
				target id:<input type="text" v-model="tx_node_id" name="tx_node_id" placeholder="target node" />
				<button @click="() => send_tx()">propagate</button>
				<br />
				<!-- <hr /> -->
				<button @click="() => easyTest()">{{ "EASY TEST" }}</button>
				<button @click="() => midTest()">{{ "MID TEST" }}</button>
				<button @click="() => hardTest()">{{ "HARD TEST" }}</button>
				<button @click="() => testLeaderOffline()">
					{{ "TEST LEADER OFFLINE: " + test_leader_offline }}
				</button>
			</div>
		</div>

		<div v-if="playground_ready" class="logger" name="the_logger">
			Event Debug Logger:
			<div name="the_logger">
				{{ the_logger }}
			</div>
			<!-- <div class="history">
				<div>
					<ul v-for="item in eventArray" v-bind:key="item.id">
						<li>{{ item }}</li>
					</ul>
				</div>
			</div> -->
		</div>
	</div>
</template>

<style scoped>
.ready {
	position: absolute;
	bottom: 0;
	right: 0;
}

.box {
	background: #1f1c2c;
	/* fallback for old browsers */
	background: -webkit-linear-gradient(to top, #1f1c2c, #928dab);
	/* Chrome 10-25, Safari 5.1-6 */
	background: linear-gradient(to top, #1f1c2c, #928dab);
	/* W3C, IE 10+/ Edge, Firefox 16+, Chrome 26+, Opera 12+, Safari 7+ */
}

.history {
	height: 300px;
	overflow: scroll;
}

.logger {
	font-family: monospace;
	padding: 30px;
	word-break: break-all;
	position: relative;
	top: 0;
	z-index: 999;
	overflow-x: hidden;
	width: auto;
	margin: 0;
	height: auto;
	box-shadow: 0px 0px 5px 3px gray inset;
	border-radius: 10px;
}

.logger div {
	position: relative;
}

.footer {
	position: relative;
	height: auto;
	width: 100%;
	background-color: darkgray;
	padding: 15px;
}

.stats {
	position: absolute;
	margin-top: 0px;
	height: auto;
	width: auto;
	background-color: darkgray;
	padding: 15px;
}

.nodeIsLeader {
	font-weight: bold;
	color: white;
	text-shadow: 0px 0px 7px black;
}

.nodeIsOffline {
	text-decoration: line-through;
}

/* exagon grid */
.nodes {
	display: flex;
	max-width: 50%;
	margin: auto;
	--s: 100px;
	--sm: 10px;
	--sn: calc(var(--s) - var(--sm));
	/* size  */
	--m: 8px;
	/* margin */
	--f: calc(1.732 * var(--s) + 4 * var(--m) - 1px);
}

.node {
	position: absolute;
	top: 0px;
	left: 0px;
	color: black;
	display: flex;
	justify-content: center;
	align-items: center;
	width: var(--sn);
	height: calc(var(--sn) * 1.1547);
	clip-path: polygon(0% 25%, 0% 75%, 50% 100%, 100% 75%, 100% 25%, 50% 0%);
	margin: calc(var(--sm) / 2);
}

.node {
	cursor: pointer;
}

.node_hover {
	background: rgba(255, 0, 0, 0.5);
}

.node_info {
	padding: 10px;
	box-shadow: 0px 0px 3px 3px white;
	position: absolute;
	bottom: 0;
	left: 0;
	width: 300px;
	height: auto;
	display: block;
}

.node_info_hide {
	display: none;
}

button {
	background: #1f1c2c;
	/* fallback for old browsers */
	background: -webkit-linear-gradient(to top, #1f1c2c, #928dab);
	/* Chrome 10-25, Safari 5.1-6 */
	background: linear-gradient(to top, #1f1c2c, #928dab);
	/* W3C, IE 10+/ Edge, Firefox 16+, Chrome 26+, Opera 12+, Safari 7+ */
}

button:disabled {
	opacity: 0.6;
	cursor: not-allowed;
	background-color: #ddd;
	color: #aaa;
}

input {
	border-radius: 10px;
}

.exagon-container {
	transform: rotate(12deg);
}

.exagon-container div {
	width: var(--s);
	margin: var(--m);
	height: calc(var(--s) * 1.1547);
	display: inline-block;
	font-size: initial;
	clip-path: polygon(0% 25%, 0% 75%, 50% 100%, 100% 75%, 100% 25%, 50% 0%);
	background: rgba(0, 255, 255, 0.5);
	box-shadow: 0px 0px 5px 5px blue;
	margin-bottom: calc(var(--m) - var(--s) * 0.2885);
	position: relative;
}

.exagon-container div:hover {
	background: rgb(0, 255, 255) !important;
	box-shadow: 0px 0px 15px 15px black;
}

.exagon-container div:nth-child(odd) {
	background: rgba(0, 255, 255, 0.3);
}

.exagon-container::before {
	content: "";
	width: calc(var(--s) / 2 + var(--m) + 0px);
	float: left;
	height: 120%;
	shape-outside: repeating-linear-gradient(#0000 0 calc(var(--f) - 3px),
			#000 0 var(--f));
}

/* endexagon grid */
/* pulse */

.vaidator {
	animation: pulse-vaidator 2s infinite;
}

@keyframes pulse-vaidator {
	0% {
		transform: scale(0.95);
		background: rgb(136, 216, 23);
		box-shadow: 0 0 0 0 rgba(255, 255, 255, 0.7);
	}

	70% {
		transform: scale(1);
		background: rgb(0, 233, 250);
		box-shadow: 0 0 0 10px rgba(255, 255, 255, 0);
	}

	100% {
		transform: scale(0.95);
		background: rgb(11, 11, 15);
		box-shadow: 0 0 0 0 rgba(255, 255, 255, 0);
	}
}

/* end pulse */
</style>
