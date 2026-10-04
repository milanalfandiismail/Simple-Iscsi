#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <winsock2.h>
#include <ws2ipdef.h>
#include <iphlpapi.h>
#include <stdio.h>
#include <stdlib.h>

#pragma comment(lib, "iphlpapi.lib")
#pragma comment(lib, "ws2_32.lib")
#pragma comment(lib, "advapi32.lib")

#define SERVICE_NAME "SimpleIscsiHelper"

FILE* g_logFile = nullptr;

void LogA(const char* fmt, ...) {
    if (!g_logFile) {
        fopen_s(&g_logFile, "C:\\helper-svc.log", "a");
        if (!g_logFile) {
            fopen_s(&g_logFile, "C:\\Windows\\System32\\helper-svc.log", "a");
        }
    }
    if (g_logFile) {
        va_list args;
        va_start(args, fmt);
        vfprintf(g_logFile, fmt, args);
        va_end(args);
        fflush(g_logFile);
    }
}

// Format IPv4 sockaddr ke string "a.b.c.d"
void FormatIpv4(const IN_ADDR* addr, char* outBuf, size_t bufSize) {
    unsigned char* b = (unsigned char*)addr;
    sprintf_s(outBuf, bufSize, "%u.%u.%u.%u", b[0], b[1], b[2], b[3]);
}

// Baca Target IP dari HKLM\SYSTEM\CurrentControlSet\Services\SimpleIscsiBoot
bool ReadTargetIpFromRegistry(char* outIp, size_t maxLen) {
    HKEY hKey = nullptr;
    if (RegOpenKeyExA(HKEY_LOCAL_MACHINE, "SYSTEM\\CurrentControlSet\\Services\\SimpleIscsiBoot", 0, KEY_READ, &hKey) == ERROR_SUCCESS) {
        char buf[64] = {0};
        DWORD bufSize = sizeof(buf);
        DWORD type = REG_SZ;
        if (RegQueryValueExA(hKey, "TargetIp", nullptr, &type, (LPBYTE)buf, &bufSize) == ERROR_SUCCESS && buf[0] != '\0') {
            strcpy_s(outIp, maxLen, buf);
            RegCloseKey(hKey);
            return true;
        }
        RegCloseKey(hKey);
    }
    return false;
}

// Fallback: Baca Target IP dari ACPI iBFT Firmware Table
bool ReadTargetIpFromFirmware(char* outIp, size_t maxLen) {
    DWORD sig = 'TFBi'; // 'iBFT'
    DWORD bufSize = GetSystemFirmwareTable('ACPI', sig, nullptr, 0);
    if (bufSize == 0) return false;

    BYTE* pBuf = (BYTE*)malloc(bufSize);
    if (!pBuf) return false;

    if (GetSystemFirmwareTable('ACPI', sig, pBuf, bufSize) == bufSize) {
        // Cari block NIC di iBFT
        for (DWORD offset = 48; offset + 16 < bufSize; offset++) {
            if (pBuf[offset] == 0x02) { // NIC structure ID
                BYTE* ipBytes = &pBuf[offset + 12];
                // Periksa apakah ini IPv4 yang dipetakan ke IPv6 (10 bytes 0x00, 2 bytes 0xFF)
                bool isMapped = true;
                for (int k = 0; k < 10; k++) {
                    if (ipBytes[k] != 0x00) { isMapped = false; break; }
                }
                if (isMapped && ipBytes[10] == 0xFF && ipBytes[11] == 0xFF) {
                    sprintf_s(outIp, maxLen, "%u.%u.%u.%u", ipBytes[12], ipBytes[13], ipBytes[14], ipBytes[15]);
                    free(pBuf);
                    return true;
                }
            }
        }
    }
    free(pBuf);
    return false;
}

// Pembersihan IP Unicast yang tidak sesuai Target IP
int ExecuteIpPurge() {
    LogA("================================================================\n");
    LogA(" [Simple-Iscsi Helper-Svc] User-Mode Network Alignment Started\n");
    LogA("================================================================\n");

    char targetIp[64] = {0};
    if (ReadTargetIpFromRegistry(targetIp, sizeof(targetIp))) {
        LogA("[+] Retrieved Target IP from SimpleIscsiBoot Registry: %s\n", targetIp);
    } else if (ReadTargetIpFromFirmware(targetIp, sizeof(targetIp))) {
        LogA("[+] Retrieved Target IP from ACPI iBFT Firmware: %s\n", targetIp);
    } else {
        LogA("[-] Error: Unable to determine Target IP from Registry or iBFT!\n");
        return 1;
    }

    // Inisialisasi Winsock untuk resolusi
    WSADATA wsaData;
    WSAStartup(MAKEWORD(2, 2), &wsaData);

    PMIB_UNICASTIPADDRESS_TABLE pTable = nullptr;
    DWORD status = GetUnicastIpAddressTable(AF_INET, &pTable);
    if (status != NO_ERROR || !pTable) {
        LogA("[-] GetUnicastIpAddressTable failed (Error: %lu)\n", status);
        WSACleanup();
        return 1;
    }

    LogA("[+] Scanning %lu active IPv4 Unicast addresses...\n", pTable->NumEntries);

    int purgedCount = 0;
    int keptCount = 0;

    for (DWORD i = 0; i < pTable->NumEntries; i++) {
        MIB_UNICASTIPADDRESS_ROW row = pTable->Table[i];
        if (row.Address.si_family == AF_INET) {
            char ipStr[64] = {0};
            FormatIpv4(&row.Address.Ipv4.sin_addr, ipStr, sizeof(ipStr));

            // Skip loopback dan APIPA
            if (strcmp(ipStr, "127.0.0.1") == 0 || strncmp(ipStr, "169.254.", 8) == 0) {
                LogA("    [*] Skipped system IP: %s\n", ipStr);
                continue;
            }

            if (strcmp(ipStr, targetIp) == 0) {
                LogA("    [+] KEEP Valid Target IP: %s (InterfaceIndex: %lu)\n", ipStr, row.InterfaceIndex);
                keptCount++;
            } else {
                LogA("    [!] DETECTED STALE/DUPLICATE IP: %s (InterfaceIndex: %lu)\n", ipStr, row.InterfaceIndex);
                LogA("        -> Purging stale IP via DeleteUnicastIpAddressEntry...\n");

                DWORD delStatus = DeleteUnicastIpAddressEntry(&row);
                if (delStatus == NO_ERROR) {
                    LogA("        [SUCCESS] Stale IP %s successfully deleted from TCP/IP stack!\n", ipStr);
                    purgedCount++;
                } else {
                    LogA("        [FAILED] DeleteUnicastIpAddressEntry failed (Error: %lu)\n", delStatus);
                }
            }
        }
    }

    FreeMibTable(pTable);
    WSACleanup();

    LogA("[+] IP Purge Complete: %d Stale IP(s) Removed, %d Target IP(s) Retained.\n", purgedCount, keptCount);
    LogA("================================================================\n");
    LogA(" [Simple-Iscsi Helper-Svc] Finished Successfully\n");
    LogA("================================================================\n\n");

    if (g_logFile) {
        fclose(g_logFile);
        g_logFile = nullptr;
    }

    return 0;
}

// Windows Service Handling
SERVICE_STATUS        g_ServiceStatus;
SERVICE_STATUS_HANDLE g_StatusHandle = nullptr;

VOID WINAPI ServiceCtrlHandler(DWORD CtrlCode) {
    if (CtrlCode == SERVICE_CONTROL_STOP || CtrlCode == SERVICE_CONTROL_SHUTDOWN) {
        g_ServiceStatus.dwCurrentState = SERVICE_STOPPED;
        SetServiceStatus(g_StatusHandle, &g_ServiceStatus);
    }
}

VOID WINAPI ServiceMain(DWORD argc, LPSTR* argv) {
    (void)argc; (void)argv;
    g_StatusHandle = RegisterServiceCtrlHandlerA(SERVICE_NAME, ServiceCtrlHandler);
    if (!g_StatusHandle) return;

    g_ServiceStatus.dwServiceType = SERVICE_WIN32_OWN_PROCESS;
    g_ServiceStatus.dwCurrentState = SERVICE_RUNNING;
    g_ServiceStatus.dwControlsAccepted = SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN;
    g_ServiceStatus.dwWin32ExitCode = 0;
    SetServiceStatus(g_StatusHandle, &g_ServiceStatus);

    // Jalankan pembersihan IP
    ExecuteIpPurge();

    // Selesai bekerja, service langsung berhenti dengan bersih (0 RAM overhead)
    g_ServiceStatus.dwCurrentState = SERVICE_STOPPED;
    SetServiceStatus(g_StatusHandle, &g_ServiceStatus);
}

int main(int argc, char* argv[]) {
    // Jika dijalankan manual atau via Run key (/run), jalankan langsung
    if (argc > 1 && (_stricmp(argv[1], "/run") == 0 || _stricmp(argv[1], "-run") == 0)) {
        return ExecuteIpPurge();
    }

    // Coba mulai sebagai Service terlebih dahulu
    SERVICE_TABLE_ENTRYA ServiceTable[] = {
        { (LPSTR)SERVICE_NAME, (LPSERVICE_MAIN_FUNCTIONA)ServiceMain },
        { NULL, NULL }
    };

    if (!StartServiceCtrlDispatcherA(ServiceTable)) {
        // Jika gagal start dispatcher (misalnya dijalankan interaktif dari double-click/cmd), jalankan direct
        return ExecuteIpPurge();
    }

    return 0;
}
