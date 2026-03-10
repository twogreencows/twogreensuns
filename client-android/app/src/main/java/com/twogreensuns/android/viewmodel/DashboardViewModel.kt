package com.twogreensuns.android.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.twogreensuns.android.network.ApiClient
import com.twogreensuns.android.network.AppConfig
import com.twogreensuns.android.network.LogLevelRequest
import com.twogreensuns.android.network.StatusResponse
import com.twogreensuns.android.BuildConfig
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

class DashboardViewModel : ViewModel() {

    private val api = ApiClient.create(BuildConfig.SERVER_URL)

    private val _status = MutableStateFlow<StatusResponse?>(null)
    val status: StateFlow<StatusResponse?> = _status

    private val _config = MutableStateFlow<AppConfig?>(null)
    val config: StateFlow<AppConfig?> = _config

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error

    init {
        refreshStatus()
    }

    fun refreshStatus() {
        viewModelScope.launch {
            runCatching { api.status() }
                .onSuccess { _status.value = it }
                .onFailure { _error.value = it.message }
        }
    }

    fun loadConfig() {
        viewModelScope.launch {
            runCatching { api.getConfig() }
                .onSuccess { _config.value = it }
                .onFailure { _error.value = it.message }
        }
    }

    fun saveConfig(cfg: AppConfig) {
        viewModelScope.launch {
            runCatching { api.putConfig(cfg) }
                .onSuccess { _config.value = cfg }
                .onFailure { _error.value = it.message }
        }
    }

    fun teslaDiscover() = launchAction { api.teslaDiscover() }
    fun teslaStart()    = launchAction { api.teslaStart() }
    fun teslaStop()     = launchAction { api.teslaStop() }
    fun teslaSetAmps(n: Int) = launchAction { api.teslaSetAmps(n) }

    private fun launchAction(block: suspend () -> Unit) {
        viewModelScope.launch {
            runCatching { block() }
                .onSuccess { refreshStatus() }
                .onFailure { _error.value = it.message }
        }
    }
}
