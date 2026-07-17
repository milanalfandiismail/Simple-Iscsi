use std::sync::Arc;
use tracing::{info, error};

use crate::config;
use crate::vhd_merge;
use crate::writeback_super;
use crate::config_manager::clear_super_client_config;

pub async fn handle_cli_args(
    args: &[String],
    config_path: &str,
    clients_path: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    if args.len() >= 2 && args[1] == "--reload" {
        info!("Reload: memvalidasi clients.toml...");
        let _clients = config::load_clients(clients_path)?;
        info!("✅ clients.toml valid! {} client(s) dimuat.", _clients.len());
        return Ok(true);
    }

    // === CLI: --restore-list <image_key> ===
    if args.len() >= 3 && args[1] == "--restore-list" {
        let image_key = &args[2];
        let config = Arc::new(config::load_config(config_path)?);

        let base_path = writeback_super::resolve_base_path(&config, image_key);
        if base_path.is_empty() {
            error!("Image key '{}' tidak ditemukan di config.toml [image_manager]", image_key);
            std::process::exit(1);
        }

        let backups = vhd_merge::list_backups(&base_path)?;

        if backups.is_empty() {
            info!("📋 Tidak ada backup untuk image '{}' ({})", image_key, base_path);
        } else {
            info!("📋 Backup untuk {}:", image_key);
            for (idx, path) in &backups {
                info!("  [{}] {}", idx, path);
            }
        }
        return Ok(true);
    }

    // === CLI: --restore <image_key> [index] ===
    if args.len() >= 3 && args[1] == "--restore" {
        let image_key = &args[2];
        let restore_idx: Option<usize> = args.get(3).and_then(|s| s.parse().ok());

        let config = Arc::new(config::load_config(config_path)?);

        let base_path = writeback_super::resolve_base_path(&config, image_key);
        if base_path.is_empty() {
            error!("Image key '{}' tidak ditemukan di config.toml [image_manager]", image_key);
            std::process::exit(1);
        }

        let backup_path = if let Some(idx) = restore_idx {
            vhd_merge::restore_backup_by_index(&base_path, idx)?
        } else {
            vhd_merge::restore_latest_backup(&base_path)?
        };

        info!("✅ Restore selesai! Base image '{}' dikembalikan dari backup.", image_key);
        info!("   Backup: {}", backup_path);

        // Hapus super VHD biar sinkron
        let super_path = writeback_super::get_super_path(&config, image_key);
        if writeback_super::super_exists(&super_path) {
            info!("   Super VHD dihapus (biar sinkron): {}", super_path);
            let _ = writeback_super::delete_super(&super_path);
        }

        return Ok(true);
    }

    // === CLI Args: --commit <hostname> atau --discard <hostname> ===
    if args.len() >= 3 && (args[1] == "--commit" || args[1] == "--discard") {
        let action = &args[1];
        let hostname = &args[2];

        // Load config
        let config = Arc::new(config::load_config(config_path)?);
        let clients = config::load_clients(clients_path)?;

        // Cari client by hostname
        let client = clients.values().find(|c| {
            c.hostname.as_deref() == Some(hostname)
        });

        let image_key = match client {
            Some(c) => c.image_manager.as_deref().unwrap_or("").to_string(),
            None => {
                error!("Client dengan hostname '{}' tidak ditemukan di clients.toml", hostname);
                std::process::exit(1);
            }
        };

        if image_key.is_empty() {
            error!("Client '{}' tidak memiliki image_manager", hostname);
            std::process::exit(1);
        }

        let base_path = writeback_super::resolve_base_path(&config, &image_key);
        let super_path = writeback_super::get_super_path(&config, &image_key);

        match action.as_str() {
            "--commit" => {
                if !writeback_super::super_exists(&super_path) {
                    error!("Super VHD untuk {} tidak ditemukan: {}", hostname, super_path);
                    std::process::exit(1);
                }

                // 1. Backup base image dulu
                let backup_path = vhd_merge::backup_before_merge(&base_path, &super_path)?;
                info!("📦 Backup created: {}", backup_path);

                // 2. Merge super VHD → base
                info!("Meng-commit super VHD {} → base {}", super_path, base_path);
                vhd_merge::merge_vhd(super_path.clone(), base_path.clone()).await?;

                // 3. Hapus super VHD
                writeback_super::delete_super(&super_path)?;

                info!("✅ Commit selesai! Image '{}' diperbarui.", image_key);
                if let Err(e) = clear_super_client_config(config_path) {
                    error!("Gagal membersihkan super_client_ip di config.toml: {}", e);
                }
            }
            "--discard" => {
                if !writeback_super::super_exists(&super_path) {
                    error!("Super VHD untuk {} tidak ditemukan: {}", hostname, super_path);
                    std::process::exit(1);
                }
                info!("Membuang super VHD: {}", super_path);
                writeback_super::delete_super(&super_path)?;
                info!("✅ Discard selesai! Perubahan di '{}' dibuang.", image_key);
                if let Err(e) = clear_super_client_config(config_path) {
                    error!("Gagal membersihkan super_client_ip di config.toml: {}", e);
                }
            }
            _ => {
                error!("Argumen tidak dikenal: {}. Gunakan --commit <hostname> atau --discard <hostname>", action);
                std::process::exit(1);
            }
        }
        return Ok(true);
    }

    Ok(false)
}
