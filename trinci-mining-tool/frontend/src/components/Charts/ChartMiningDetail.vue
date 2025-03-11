<script setup lang="ts">
import { ref } from 'vue'
// @ts-ignore
import VueApexCharts from 'vue3-apexcharts'
import { useNodeStore } from '@/stores/node'
import { storeToRefs } from 'pinia';
import { ArrowDownIcon, ArrowUpIcon } from '@heroicons/vue/24/solid';



const { nodeData } = storeToRefs(useNodeStore());

const chartData = ref({
  series: [
    {
      name: 'Total Investment',
      data: nodeData?.value?.stats?.bitbel?.map(btb => [btb.time * 1000, btb.units])
    }
  ]
})

const chart = ref(null)

const apexOptions = {
  colors: ['#3C50E0'],
  chart: {
    fontFamily: 'Satoshi, sans-serif',
    height: 310,
    id: 'area-datetime',
    type: 'area',
    toolbar: {
      show: false
    }
  },
  legend: {
    show: false,
    position: 'top',
    horizontalAlign: 'left'
  },
  stroke: {
    curve: 'straight',
    width: ['1', '1']
  },

  dataLabels: {
    enabled: false
  },

  markers: {
    size: 0
  },

  labels: {
    show: false,
    position: 'top'
  },
  xaxis: {
    type: 'datetime',
    tickAmount: 10,
    axisBorder: {
      show: false
    },
    axisTicks: {
      show: false
    }
  },

  tooltip: {
    x: {
      format: 'dd MMM yyyy'
    }
  },
  fill: {
    gradient: {
      enabled: true,
      opacityFrom: 0.55,
      opacityTo: 0
    }
  },

  grid: {
    strokeDashArray: 7,
    xaxis: {
      lines: {
        show: true
      }
    },
    yaxis: {
      lines: {
        show: true
      }
    }
  },
  responsive: [
    {
      breakpoint: 1024,
      options: {
        chart: {
          height: 300
        }
      }
    },
    {
      breakpoint: 1366,
      options: {
        chart: {
          height: 320
        }
      }
    }
  ]
}

const last = <T>(a: T[]) => {
  return a[a.length - 1];
}

const percentage = <T>(a: T[],filed:string) => {
  const lastElement = last<any>(a)
  const first = a[0] as any;
  return (first[filed] && first[filed] !==0) ? +(((lastElement[filed] - first[filed])/first[filed])*100).toFixed(2): 0
}
</script>

<template>
  <div
    class="col-span-12 rounded-xl border border-stroke bg-white px-5 pb-5 pt-7.5 shadow-default dark:border-strokedark dark:bg-boxdark sm:px-7.5 xl:col-span-7"
  >
    <div class="mb-5.5 flex flex-wrap items-center justify-between gap-2">
      <div>
        <h4 class="text-title-sm2 font-bold text-black dark:text-white">Mining rewards</h4>
      </div>
      <div class="relative z-20 inline-block rounded">
        <!-- <select
          class="relative z-20 inline-flex appearance-none rounded border border-stroke bg-transparent py-[5px] pl-3 pr-8 text-sm font-medium outline-none dark:border-strokedark"
        >
          <option value="" class="dark:bg-boxdark">Last 7 days</option>
          <option value="" class="dark:bg-boxdark">Last 15 days</option>
        </select>
        <span class="absolute right-3 top-1/2 z-10 -translate-y-1/2">
          <svg
            width="17"
            height="17"
            viewBox="0 0 17 17"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
          >
            <path
              d="M8.61025 11.8872C8.46025 11.8872 8.33525 11.8372 8.21025 11.7372L2.46025 6.08723C2.23525 5.86223 2.23525 5.51223 2.46025 5.28723C2.68525 5.06223 3.03525 5.06223 3.26025 5.28723L8.61025 10.5122L13.9603 5.23723C14.1853 5.01223 14.5353 5.01223 14.7603 5.23723C14.9853 5.46223 14.9853 5.81223 14.7603 6.03723L9.01025 11.6872C8.88525 11.8122 8.76025 11.8872 8.61025 11.8872Z"
              fill="#64748B"
            />
          </svg>
        </span> -->
      </div>
    </div>

    <div class="mb-3 flex flex-wrap gap-6">
      <div>
        <p class="mb-1.5 text-sm font-medium">TCoin balance</p>
        <div class="flex items-center gap-2.5">
          <p class="font-medium text-black dark:text-white">{{ last<any>(nodeData?.stats?.tcoin || [{units:0 , time: 0}]).total}}</p>
          <p :class="{
            'flex items-center gap-1 font-medium': true,
            'text-meta-3': percentage<any>(nodeData?.stats?.tcoin || [{units: 0, time: 0}], 'units_in_stake') > 0,
            'text-red': percentage<any>(nodeData?.stats?.tcoin || [{units: 0, time: 0}], 'units_in_stake') < 0
            }">
            {{ percentage<any>(nodeData?.stats?.tcoin || [{units: 0, time: 0}], 'units_in_stake') }}%

            <ArrowUpIcon v-if="percentage<any>(nodeData?.stats?.tcoin || [{units: 0, time: 0}], 'units_in_stake') > 0" class="text-meta-3 h-4"/>
            <ArrowDownIcon v-if="percentage<any>(nodeData?.stats?.tcoin || [{units: 0, time: 0}], 'units_in_stake') < 0" class="text-red h-4"/>

<!--             <svg
              class="fill-current"
              width="11"
              height="8"
              viewBox="0 0 11 8"
              fill="none"
              xmlns="http://www.w3.org/2000/svg"
            >
              <path
                d="M5.77105 0.0465078L10.7749 7.54651L0.767256 7.54651L5.77105 0.0465078Z"
                fill=""
              />
            </svg> -->
          </p>
        </div>
      </div>

      <div>
        <p class="mb-1.5 text-sm font-medium">Bitbel balance</p>
        <div class="flex items-center gap-2.5">
          <p class="font-medium text-black dark:text-white">{{ last(nodeData?.stats?.bitbel || [{units: 0, time: 0}]).units }}</p>
          <p :class="{
            'flex items-center gap-1 font-medium': true,
            'text-meta-3': percentage<any>(nodeData?.stats?.bitbel || [{units: 0, time: 0}], 'units') > 0,
            'text-red': percentage<any>(nodeData?.stats?.bitbel || [{units: 0, time: 0}], 'units') < 0
            }">
            {{ percentage<any>(nodeData?.stats?.bitbel || [{units: 0, time: 0}], 'units') }}%

            <ArrowUpIcon v-if="percentage<any>(nodeData?.stats?.bitbel || [{units: 0, time: 0}], 'units') > 0" class="text-meta-3 h-4" />
            <ArrowDownIcon v-if="percentage<any>(nodeData?.stats?.bitbel || [{units: 0, time: 0}], 'units') < 0" class="text-red h-4" />

<!--             <svg
              class="fill-current"
              width="11"
              height="8"
              viewBox="0 0 11 8"
              fill="none"
              xmlns="http://www.w3.org/2000/svg"
            >
              <path
                d="M5.77105 0.0465078L10.7749 7.54651L0.767256 7.54651L5.77105 0.0465078Z"
                fill=""
              />
            </svg> -->
          </p>
        </div>
      </div>
    </div>
    <div>
      <div id="chartThirteen" class="-ml-5">
        <VueApexCharts
          type="area"
          height="310"
          :options="apexOptions"
          :series="chartData.series"
          ref="chart"
        />
      </div>
    </div>
  </div>
</template>
