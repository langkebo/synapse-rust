(function() {
  var style = getComputedStyle(document.documentElement);
  var accent = style.getPropertyValue('--accent').trim();
  var accent2 = style.getPropertyValue('--accent2').trim();
  var ink = style.getPropertyValue('--ink').trim();
  var muted = style.getPropertyValue('--muted').trim();
  var rule = style.getPropertyValue('--rule').trim();
  var bg2 = style.getPropertyValue('--bg2').trim();
  var warn = style.getPropertyValue('--warn').trim();

  // --- Chart 1: Test Distribution ---
  var chart1 = echarts.init(document.getElementById('chart-distribution'), null, { renderer: 'svg' });
  chart1.setOption({
    animation: false,
    tooltip: {
      trigger: 'item',
      appendToBody: true,
      formatter: '{b}: {c} ({d}%)'
    },
    legend: {
      orient: 'horizontal',
      bottom: 0,
      textStyle: { color: muted, fontSize: 13 },
      itemGap: 20
    },
    series: [{
      type: 'pie',
      radius: ['40%', '70%'],
      center: ['50%', '45%'],
      avoidLabelOverlap: true,
      itemStyle: {
        borderRadius: 6,
        borderColor: bg2,
        borderWidth: 2
      },
      label: {
        show: true,
        formatter: '{b}\n{c}',
        color: ink,
        fontSize: 13,
        fontWeight: 600
      },
      labelLine: {
        lineStyle: { color: rule }
      },
      data: [
        { value: 505, name: '库内联测试', itemStyle: { color: accent } },
        { value: 1755, name: '单元测试', itemStyle: { color: accent2 } },
        { value: 0, name: '文档测试', itemStyle: { color: muted } }
      ]
    }]
  });
  window.addEventListener('resize', function() { chart1.resize(); });

  // --- Chart 2: Fix Types ---
  var chart2 = echarts.init(document.getElementById('chart-fixes'), null, { renderer: 'svg' });
  chart2.setOption({
    animation: false,
    tooltip: {
      trigger: 'axis',
      axisPointer: { type: 'shadow' },
      appendToBody: true
    },
    grid: {
      left: '3%',
      right: '8%',
      bottom: '3%',
      top: '8%',
      containLabel: true
    },
    xAxis: {
      type: 'value',
      max: 4,
      axisLabel: { color: muted, fontSize: 12 },
      splitLine: { lineStyle: { color: rule, type: 'dashed' } }
    },
    yAxis: {
      type: 'category',
      data: ['迁移测试修复', '路由固件更新', 'Clippy 修复'],
      axisLabel: { color: ink, fontSize: 13 },
      axisLine: { lineStyle: { color: rule } },
      axisTick: { show: false }
    },
    series: [{
      type: 'bar',
      barWidth: '50%',
      itemStyle: {
        borderRadius: [0, 6, 6, 0],
        color: function(params) {
          var colors = [warn, accent, accent2];
          return colors[params.dataIndex];
        }
      },
      label: {
        show: true,
        position: 'right',
        color: ink,
        fontSize: 13,
        fontWeight: 600,
        formatter: '{c} 个'
      },
      data: [1, 3, 1]
    }]
  });
  window.addEventListener('resize', function() { chart2.resize(); });
})();
