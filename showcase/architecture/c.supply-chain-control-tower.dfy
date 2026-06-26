// Supply chain control tower spanning planning, fulfillment, logistics, and visibility
// Mermaid mapping: complex architecture graph for cross-enterprise supply chain coordination
diagram architecture {
    title: "供应链控制塔"

    group upstream "上游与计划" {
        entity supplier "Supplier Portal" { type: external }
        entity procurement "Procurement Service" { type: service }
        entity planning "Demand Planning" { type: service }
        entity forecast "Forecast Engine" { type: service }
    }

    group operations "运营与履约" {
        entity order_mgmt "Order Management" { type: service }
        entity inventory "Inventory Service" { type: service }
        entity warehouse_ops "Warehouse Operations" { type: service }
        entity allocation "Allocation Engine" { type: service }
    }

    group logistics "物流协同" {
        entity carrier "Carrier Network" { type: external }
        entity shipment "Shipment Service" { type: service }
        entity tracking "Tracking Hub" { type: service }
        entity eta "ETA Predictor" { type: service }
    }

    group visibility "控制塔能力" {
        entity event_bus "Supply Event Bus" { type: queue }
        entity control_tower "Control Tower UI" { type: frontend }
        entity alerting "Alerting Engine" { type: service }
        entity analytics "Operations Analytics" { type: database }
    }

    supplier -> procurement
    procurement -> planning
    planning -> forecast
    forecast -> order_mgmt "replenishment plan"
    order_mgmt -> allocation
    allocation -> inventory
    inventory -> warehouse_ops
    warehouse_ops -> shipment "release shipment"
    shipment -> carrier
    carrier --> tracking "tracking updates"
    tracking -> eta
    procurement -> event_bus "po events"
    order_mgmt -> event_bus "fulfillment events"
    tracking -> event_bus "logistics events"
    event_bus -> alerting
    event_bus -> analytics
    analytics -> control_tower
    alerting -> control_tower
}
