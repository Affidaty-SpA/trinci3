<script setup lang="ts">
import { ref, watchPostEffect } from 'vue'
// @ts-ignore
import VueApexCharts from 'vue3-apexcharts';
import { useNodeStore } from "@/stores/node";
import { InformationCircleIcon } from '@heroicons/vue/24/outline';
import { storeToRefs } from 'pinia';

const { miningToolPerformance } = storeToRefs(useNodeStore());

const chartData = ref({
  series: [
    {
      name: 'CPU',
      data: [0]
    },
    {
      name: 'Memory',
      data: [0]
    },
    {
      name: 'Disk',
      data: [0]
    }
  ],
  labels: [`${new Date().getHours()}:${new Date().getMinutes()}:${new Date().getSeconds()}`]
})

const chart = ref(null)

const apexOptions = {
  legend: {
    show: false,
    position: 'top',
    horizontalAlign: 'left'
  },
  colors: ['#B7EB71', '#C7D2E2', '#68F5FF'], // QUI I COLORI DELLE LINEE
  chart: {
    fontFamily: 'Satoshi, sans-serif',
    height: 310,
    type: 'area',
    toolbar: {
      show: false
    }
  },
  fill: {
    gradient: {
      enabled: true,
      opacityFrom: 0.55,
      opacityTo: 0
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
  ],
  stroke: {
    curve: 'smooth',
    width: ['3.5', '3.5']
  },

  markers: {
    size: 0
  },
  labels: {
    show: false,
    position: 'top'
  },
  grid: {
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
  dataLabels: {
    enabled: false
  },
  tooltip: {
    fixed: {
      enabled: !1
    },
    x: {
      show: !1
    },
    y: {
      title: {
        formatter: function () {
          return ''
        }
      }
    },
    marker: {
      show: !1
    }
  },
  xaxis: {
    type: 'category',
    categories: chartData.value.labels,
    axisBorder: {
      show: false
    },
    axisTicks: {
      show: false
    },
    labels: {
      show: false,
      rotate: -45,
      /* datetimeFormatter: {
        year: 'yyyy',
        month: "MMM 'yy",
        day: 'dd MMM',
        hour: 'HH:mm',
      },
      datetimeUTC: true, */
      //formatter: (val) => {console.log(val)}
    }
  },
  yaxis: {
    min:0,
    max: 100,
    title: {
      style: {
        fontSize: '0px'
      }
    }
  }
}

const refreshData = () => {
  chartData.value.series[0].data = miningToolPerformance.value.map((val) => {
    return val.cpus_usage.total != 0 ? Math.round((val.cpus_usage.used / val.cpus_usage.total) * 100) : 0
  })
  chartData.value.series[1].data = miningToolPerformance.value.map((val) => {
    return val.mem_usage.total != 0 ? Math.round((val.mem_usage.used / val.mem_usage.total) * 100) : 0
  })
  chartData.value.series[2].data = miningToolPerformance.value.map((val) => {
    return val.disk_usage.total != 0 ? Math.round((val.disk_usage.used / val.disk_usage.total) * 100) : 0
  })
  /*chartData.value.labels = miningToolPerformance.value.map(val => {
    const h = new Date(val.time!);
    return `${h.getHours()}:${h.getMinutes()}:${h.getSeconds()}`
  });*/
}

watchPostEffect(() => {
  if (miningToolPerformance.value) {
    refreshData();
  }
})
</script>

<template>
  <div
    class=" relative col-span-12 rounded-xl border border-stroke bg-white px-5 pt-7.5 pb-5 shadow-default dark:border-strokedark dark:bg-boxdark sm:px-7.5 xl:col-span-8"
  >
  <button class="absolute right-3 top-3">
          <InformationCircleIcon class=" text-mining-blue hover:text-mining-green w-6 h-6 "/>
        </button>
    <div class="mb-6 flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
      <div>
        <h4 class="text-title-sm2 font-bold text-black dark:text-white mt-2">Mining tool performance</h4>
      </div>
      <!-- <div class="flex items-center">
        <p class="font-medium uppercase text-black dark:text-white">Short by:</p>
        <div class="relative z-20 inline-block">
          <select
            name="#"
            id="#"
            class="relative z-20 inline-flex appearance-none bg-transparent py-1 pl-3 pr-8 font-medium outline-none"
          >
            <option value="" class="dark:bg-boxdark">Monthly</option>
            <option value="" class="dark:bg-boxdark">Weekly</option>
          </select>
          <span class="absolute top-1/2 right-1 z-10 -translate-y-1/2">
            <svg
              width="18"
              height="18"
              viewBox="0 0 18 18"
              fill="none"
              xmlns="http://www.w3.org/2000/svg"
            >
              <path
                d="M8.99995 12.8249C8.8312 12.8249 8.69058 12.7687 8.54995 12.6562L2.0812 6.2999C1.82808 6.04678 1.82808 5.65303 2.0812 5.3999C2.33433 5.14678 2.72808 5.14678 2.9812 5.3999L8.99995 11.278L15.0187 5.34365C15.2718 5.09053 15.6656 5.09053 15.9187 5.34365C16.1718 5.59678 16.1718 5.99053 15.9187 6.24365L9.44995 12.5999C9.30933 12.7405 9.1687 12.8249 8.99995 12.8249Z"
                fill="#64748B"
              />
            </svg>
          </span>
        </div>
      </div> -->
    </div>
    <div>
      <div id="chartSeven" class="-ml-5">
        <VueApexCharts
          type="area"
          height="310"
          :options="apexOptions"
          :series="chartData.series"
          ref="chart"
        />
      </div>
    </div>

    <div class="flex flex-col  xsm:flex-row">
      <div class="py-2 xsm:w-1/2 flex items-center gap-5">
        <div class=" bg-[#B7EB71] h-1 w-30 rounded"></div>
        <p class="font-medium">CPU/t</p>
      </div>
      <div class="py-2 xsm:w-1/2 flex items-center gap-5">
        <div class=" bg-[#C7D2E2] h-1 w-30 rounded"></div>
        <p class="font-medium">Memory/t</p>
      </div>
      <div class="py-2 xsm:w-1/2 flex items-center gap-5">
        <div class=" bg-[#68F5FF] h-1 w-30 rounded"></div>
        <p class="font-medium">Disk/t</p>
      </div>
    </div>
  </div>
</template>
