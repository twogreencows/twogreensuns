package com.twogreensuns.android.ui.screens

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.twogreensuns.android.network.InverterData
import com.twogreensuns.android.network.TeslaState
import com.twogreensuns.android.viewmodel.DashboardViewModel

@Composable
fun DashboardScreen(viewModel: DashboardViewModel) {
    val status by viewModel.status.collectAsState()

    Column(modifier = Modifier.fillMaxSize().padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Text("Dashboard", style = MaterialTheme.typography.headlineMedium)

        status?.inverter?.let { InverterCard(it) }
            ?: Card(modifier = Modifier.fillMaxWidth()) {
                Box(Modifier.padding(16.dp)) {
                    Text("No inverter data yet — waiting for first poll.", color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }

        status?.tesla?.let { TeslaCard(it, viewModel) }
    }
}

@Composable
private fun InverterCard(data: InverterData) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Solar Inverter", style = MaterialTheme.typography.titleMedium)
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                PowerMetric("Solar (PV)",  data.pPv)
                PowerMetric("House load", data.pLoad)
                PowerMetric("Grid",       data.pGrid)
                PowerMetric("Battery",    data.pAkku)
            }
        }
    }
}

@Composable
private fun PowerMetric(label: String, watts: Double) {
    Column(horizontalAlignment = androidx.compose.ui.Alignment.CenterHorizontally) {
        Text(label, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text("${watts.toInt()} W", style = MaterialTheme.typography.titleSmall)
    }
}

@Composable
private fun TeslaCard(state: TeslaState, viewModel: DashboardViewModel) {
    var targetAmps by remember { mutableIntStateOf(8) }

    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                Text("Tesla", style = MaterialTheme.typography.titleMedium)
                Badge(containerColor = if (state.connected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceVariant) {
                    Text(if (state.connected) "Connected" else "Disconnected")
                }
            }

            Text("Charging: ${if (state.charging) "Yes" else "No"}  •  ${state.chargingAmps} A",
                style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)

            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = { viewModel.teslaStart() }) { Text("Start") }
                OutlinedButton(onClick = { viewModel.teslaStop() }) { Text("Stop") }
                OutlinedButton(onClick = { viewModel.teslaDiscover() }) { Text("Discover") }
            }

            Column {
                Text("Set amps: $targetAmps A", style = MaterialTheme.typography.bodySmall)
                Slider(value = targetAmps.toFloat(), onValueChange = { targetAmps = it.toInt() },
                    valueRange = 1f..16f, steps = 14)
                Button(onClick = { viewModel.teslaSetAmps(targetAmps) }) { Text("Apply") }
            }
        }
    }
}
