<script setup lang="ts">
import { ref, watchPostEffect } from 'vue'
// @ts-ignore
import VueApexCharts from 'vue3-apexcharts';
import { useNodeStore } from "@/stores/node";
import { InformationCircleIcon } from '@heroicons/vue/24/outline';
import { storeToRefs } from 'pinia';

const { allNodesData } = storeToRefs(useNodeStore());

const chartData = ref<any>({ // TODO: Remove this any
  series: [
    {
      data: []
    }
  ],
  labels: []
})

const chart = ref(null)

const refreshData = () => {
    const allTimes =  [...allNodesData.value.values()].reduce((acc,node) => {
    node.stats.bitbel.forEach(el => acc.add(el.time))
    return acc;
  }, new Set<number>()); // block time list

  const allTimeArray = [...allTimes].sort();//

  const bitbelSeries = allTimeArray.reduce((acc, time) => {
    [...allNodesData.value.values()].forEach(node => {
      const bitbelInperiod = node.stats.bitbel.find(t => t.time == time);
      if(bitbelInperiod) {
        if(acc.has(time)) {
          acc.set(time,acc.get(time)! + bitbelInperiod.units)
        } else {
          acc.set(time,bitbelInperiod.units);
        }
      }
    })
    return acc;
  }, new Map<number,number>())
    //chartData.value.labels = allTimeArray.map(t => new Date(t*1000).getDate()) as string[];
    chartData.value.series[0].data = [...bitbelSeries.values()] as number[]
  }

const apexOptions = {
  colors: ['#B7EB71'],
  chart: {
    fontFamily: 'Satoshi, sans-serif',
    type: 'bar',
    height: 350,
    toolbar: {
      show: false
    }
  },
  plotOptions: {
    bar: {
      horizontal: false,
      columnWidth: '55%',
      endingShape: 'rounded',
      borderRadius: 2
    }
  },
  dataLabels: {
    enabled: false
  },
  stroke: {
    show: true,
    width: 4,
    colors: ['transparent']
  },
  xaxis: {
    categories: chartData.value.labels,
    axisBorder: {
      show: false
    },
    axisTicks: {
      show: false
    }
  },
  legend: {
    show: true,
    position: 'top',
    horizontalAlign: 'left',
    fontFamily: 'Satoshi',

    markers: {
      radius: 99
    }
  },
  yaxis: {
    title: false
  },
  grid: {
    yaxis: {
      lines: {
        show: false
      }
    }
  },
  fill: {
    opacity: 1
  },
  tooltip: {
    x: {
      show: false
    },
    y: {
      formatter: function (val: any) {
        return val
      }
    }
  }
}

watchPostEffect(() => {
  if (allNodesData.value) {
    refreshData();
  }
})
</script>

<template>
  <div
    class="relative col-span-12 rounded-xl border border-stroke bg-white px-5 pt-7.5 pb-5 shadow-default dark:border-strokedark dark:bg-boxdark sm:px-7.5"
  >
  <button class="absolute right-3 top-3">
    <InformationCircleIcon class=" text-mining-blue hover:text-mining-green w-6 h-6 "/>
  </button>
    <div>
      <h3 class="text-xl font-bold text-black dark:text-white mt-2">Mining rewards</h3>
    </div>
    <div>
      <div id="chartFour" class="-ml-5">
        <VueApexCharts
          type="bar"
          height="335"
          :options="apexOptions"
          :series="chartData.series"
          ref="chart"
        />
      </div>
    </div>

    <div class="">
      <div class="py-2 dark:border-strokedark flex items-center gap-5">
        <div class=" bg-mining-green h-1 w-30 rounded"></div>
        <div>
          <p class="font-medium">Bitbel/t</p>
          <p class="text-sm text-mining-gray-3">Bitbel earned by my players</p>
         </div>
      </div>
    </div>
  </div>
</template>
