<script setup lang="ts">
import { onMounted, ref, toRefs, watchPostEffect } from 'vue'
// @ts-ignore
import VueApexCharts from 'vue3-apexcharts';
import { InformationCircleIcon } from '@heroicons/vue/24/outline';
import { storeToRefs } from 'pinia';
import { useNodeStore } from '@/stores/node';

const props = defineProps<{
  accountId: string;
}>();

const { accountId } = toRefs(props)
const { nodeByAccountId, nodeData } = storeToRefs(useNodeStore());

onMounted(() => {
  if (nodeData?.value && Object.keys(nodeData.value).length > 0) {
    console.log(nodeData.value)
    refreshData()
  }
})

const chartData = ref({
  series: [
    {
      data: Array(3).fill(0)
    }
  ],
   labels: [
    ' ', ' ', ' '
    // `${nodeData?.value?.health?.system_stats?.mem_usage.used}/${nodeData?.value?.health?.system_stats?.mem_usage.total} ${nodeData?.value?.health?.system_stats?.mem_usage.measure}`,
    // `${nodeData?.value?.health?.system_stats?.disk_usage.used}/${nodeData?.value?.health?.system_stats?.disk_usage.total} ${nodeData?.value?.health?.system_stats?.disk_usage.measure}`,
    // `${nodeData?.value?.health?.system_stats?.cpus_usage.used}/${nodeData?.value?.health?.system_stats?.cpus_usage.total} ${nodeData?.value?.health?.system_stats?.cpus_usage.measure}`,
  ]
})

const refreshData = () => {
  if (nodeData?.value !== undefined && Object.keys(nodeData?.value || {}).length > 0) {
    chartData.value.series[0].data = [
      parseInt(((nodeData.value?.health?.system_stats?.mem_usage?.used / nodeData.value?.health?.system_stats?.mem_usage?.total) * 100).toFixed(0)), //memory
      parseInt(((nodeData.value?.health?.system_stats?.disk_usage?.used / nodeData.value?.health?.system_stats?.disk_usage?.total) * 100).toFixed(0)), //disk
      parseInt(((nodeData.value?.health?.system_stats?.cpus_usage?.used / nodeData.value?.health?.system_stats?.cpus_usage?.total) * 100).toFixed(0)), //cpu
    ]
  }
}


watchPostEffect(() => {
  if (Object.keys(nodeData?.value || {}).length > 0) {
    refreshData();
  }
})

const chart = ref(null)

const apexOptions = {
  // colors: ['#B7EB71'],
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
    title: false,
    min: 0,             // Valore minimo sull'asse Y
    max: 100,           // Valore massimo sull'asse Y
    tickAmount: 6,      // Numero di tick sull'asse Y
  },
  grid: {
    yaxis: {
      lines: {
        show: false
      }
    }
  },
  fill: {
    type: 'gradient',
    gradient: {
      shade: 'light',
      type: 'vertical',
      gradientToColors: ['#e51f1f', '#44ce1b'],
      opacityFrom: 1,
      opacityTo: 1,
      stops: [5, 100],
    },
  },
  tooltip: {
    x: {
      show: false
    },
    y: {
      //   title: {
      //   formatter: (seriesName) => { console.log(seriesName); return 'ciao bello'}
      // },
      formatter: function (val: any) {
        return `Used: ${val}`
      }
    }
  }
}
</script>

<template>
  <div
    class="min-h-[494px] relative col-span-12 rounded-xl border border-stroke bg-white px-5 pt-7.5 pb-5 shadow-default dark:border-strokedark dark:bg-boxdark sm:px-7.5"
  >
  <button class="absolute right-3 top-3">
          <InformationCircleIcon class=" text-mining-blue hover:text-mining-green w-6 h-6 "/>
        </button>
    <div>
      <h3 class="text-xl font-bold text-black dark:text-white mt-2">Node performance</h3>
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

    <div class="grid grid-cols-3 ml-11 text-sm">
      <div>Memory <span class="text-mining-gray-3 block">usage</span></div>
      <div>Disk <span class="text-mining-gray-3 block">space</span></div>
      <div>CPU <span class="text-mining-gray-3 block">usage</span></div>
    </div>
  </div>
</template>
