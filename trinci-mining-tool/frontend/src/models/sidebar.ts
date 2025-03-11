export interface IMenuItem {
  node_indicator_1: boolean
  node_indicator_2: boolean
  node_indicator_3: boolean
  node_id: string
  node_alignment: boolean
  node_value_stake_a: number
  node_value_stake_b: number
  node_value_bitbel: number
  route: string
  node_classes: string // type "player" or "npc"
  node_disabled: boolean
  node_selected: boolean
}

export interface IMenuGroup {
  name?: string
  menuItems: IMenuItem[]
}
