package com.twogreensuns.android.ui.screens

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.twogreensuns.android.network.AppConfig
import com.twogreensuns.android.viewmodel.DashboardViewModel

@Composable
fun SettingsScreen(viewModel: DashboardViewModel) {
    val config by viewModel.config.collectAsState()

    LaunchedEffect(Unit) { viewModel.loadConfig() }

    if (config == null) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            CircularProgressIndicator()
        }
        return
    }

    var draft by remember(config) { mutableStateOf(config!!) }

    Column(modifier = Modifier.fillMaxSize().padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Text("Settings", style = MaterialTheme.typography.headlineMedium)

        Card(modifier = Modifier.fillMaxWidth()) {
            Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Inverter", style = MaterialTheme.typography.titleMedium)
                OutlinedTextField(
                    value = draft.inverterIpv4Address,
                    onValueChange = { draft = draft.copy(inverterIpv4Address = it) },
                    label = { Text("IP Address") },
                    modifier = Modifier.fillMaxWidth(),
                )
                OutlinedTextField(
                    value = draft.pollingIntervalMinutes.toString(),
                    onValueChange = { draft = draft.copy(pollingIntervalMinutes = it.toIntOrNull() ?: draft.pollingIntervalMinutes) },
                    label = { Text("Poll interval (minutes)") },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }

        Card(modifier = Modifier.fillMaxWidth()) {
            Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Tesla", style = MaterialTheme.typography.titleMedium)
                OutlinedTextField(
                    value = draft.teslaVin ?: "",
                    onValueChange = { draft = draft.copy(teslaVin = it.ifBlank { null }) },
                    label = { Text("VIN (optional)") },
                    modifier = Modifier.fillMaxWidth(),
                )
                OutlinedTextField(
                    value = draft.teslaMaxAmps.toString(),
                    onValueChange = { draft = draft.copy(teslaMaxAmps = it.toIntOrNull() ?: draft.teslaMaxAmps) },
                    label = { Text("Max amps") },
                    modifier = Modifier.fillMaxWidth(),
                )
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween,
                    modifier = Modifier.fillMaxWidth()) {
                    Text("Use mock BLE (testing)")
                    Switch(checked = draft.useMockBle, onCheckedChange = { draft = draft.copy(useMockBle = it) })
                }
            }
        }

        Button(onClick = { viewModel.saveConfig(draft) }, modifier = Modifier.fillMaxWidth()) {
            Text("Save")
        }
    }
}
