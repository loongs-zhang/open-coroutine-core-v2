fn main() {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "linux")] {
            use xdpsock::{
                socket::{BindFlags, SocketConfig, SocketConfigBuilder, XdpFlags},
                umem::{UmemConfig, UmemConfigBuilder},
                xsk::Xsk2,
            };

            //todo 获取网卡设备名称
            let ifname = "eth0";

            // Configuration
            let umem_config = UmemConfigBuilder::new()
                .frame_count(8192)
                .comp_queue_size(4096)
                .fill_queue_size(4096)
                .build()
                .unwrap();

            let socket_config = SocketConfigBuilder::new()
                .tx_queue_size(4096)
                .rx_queue_size(4096)
                .bind_flags(BindFlags::XDP_COPY) // Check your driver to see if you can use ZERO_COPY
                .xdp_flags(XdpFlags::XDP_FLAGS_SKB_MODE) // Check your driver to see if you can use DRV_MODE
                .build()
                .unwrap();

            let n_tx_frames = umem_config.frame_count() / 2;

            let mut xsk = Xsk2::new(ifname, 0, umem_config, socket_config, n_tx_frames as usize);

            // Sending a packet
            let pkt: Vec<u8> = vec![];
            xsk.send(&pkt);

            // Receiving a packet
            let (_recvd_pkt, _len) = xsk.recv().expect("failed to recv");
        }
    }
}
