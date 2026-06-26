// Converted from D2 — cell tower network benchmark
// Purpose: visual parity baseline for comparing Drawify vs D2 rendering
// D2 source: https://d2lang.com/ (official tutorial-style network diagram)
//
// Original D2:
//   vars: {
//     d2-config: {
//       layout-engine: elk
//       theme-id: 300          # Terminal theme
//     }
//   }
//   network: {
//     cell tower: {
//       satellites: { shape: stored_data, style.multiple: true }
//       transmitter
//       satellites -> transmitter: send
//       satellites -> transmitter: send
//       satellites -> transmitter: send
//     }
//     online portal: { ui: {shape: hexagon} }
//     data processor: { storage: { shape: cylinder, style.multiple: true } }
//     cell tower.transmitter -> data processor.storage: phone logs
//   }
//   user: { shape: person, width: 130 }
//   user -> network.cell tower: make call
//   user -> network.online portal.ui: access { style.stroke-dash: 3 }
//   api server -> network.online portal.ui: display
//   api server -> logs: persist
//   logs: {shape: page; style.multiple: true}
//   network.data processor -> api server
//
// Conversion notes (lossy mappings):
//   - D2 vars.layout-engine elk  -> layout_algo: architecture
//   - D2 theme-id 300 (Terminal) -> theme: common.clean-dark (approximate)
//   - D2 style.multiple          -> not supported; single node per id
//   - D2 shape stored_data/page  -> type: storage + rounded_rect
//   - D2 container-level edges   -> routed to representative child entity
//   - D2 three identical send edges -> collapsed to one relation

diagram architecture {
    title: "Cell Tower Network (from D2)"
    config {
        layout: architecture
        theme: common.clean-dark
    }

    entity user "User" {
        type: frontend
        semantic: user
        style.width: 130
    }

    entity api_server "API Server" { type: service }

    entity logs "Logs" {
        type: storage
        style.shape: rounded_rect
    }

    group network "Network" {
        group cell_tower "Cell Tower" {
            entity satellites "Satellites" {
                type: storage
                style.shape: rounded_rect
            }
            entity transmitter "Transmitter" { type: service }
        }

        group online_portal "Online Portal" {
            entity ui "UI" {
                type: gateway
                style.shape: hexagon
            }
        }

        group data_processor "Data Processor" {
            entity storage "Storage" {
                type: database
                style.shape: cylinder
            }
        }
    }

    // D2: satellites -> transmitter: send (×3, style.multiple)
    satellites -> transmitter "send"

    // D2: cell tower.transmitter -> data processor.storage
    transmitter -> storage "phone logs"

    // D2: user -> network.cell tower (container target -> transmitter)
    user -> transmitter "make call"

    // D2: user -> network.online portal.ui { style.stroke-dash: 3 }
    user -> ui "access" {
        style.stroke_dasharray: "3"
    }

    api_server -> ui "display"
    api_server -> logs "persist"

    // D2: network.data processor -> api server (container source -> storage)
    storage -> api_server
}
