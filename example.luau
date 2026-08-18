-- Services
local TweenService = game:GetService("TweenService")

-- Modules
local framework = require("../../framework")

--Props
export type Props = {
	alignement: "HORIZONTAL" | "VERTICAL",
	color: Color3?,
	duration: number?,
    waiting_time: number?,
}

return function(parent: GuiObject, props: Props)
	local container = framework.Container(parent, {
		BgTransparency = 1,
		Size = UDim2.fromScale(1, 1)
	})

	local isHoriz = props.alignement == "HORIZONTAL"
	local propsFirstPos = UDim2.fromScale(0, 0)
	local propsSecondPos = if isHoriz then UDim2.fromScale(0, .5) else UDim2.fromScale(.5, 0)
	local panelSize = if isHoriz then UDim2.fromScale(1, .5) else UDim2.fromScale(.5, 1)
	local firstEndPos = if isHoriz then UDim2.fromScale(0, -.5) else UDim2.fromScale(-.5, 0)
	local secondEndPos = if isHoriz then UDim2.fromScale(0, 1) else UDim2.fromScale(1, 0)

	local firstPanel = framework.Frame(container, {
		Position = firstEndPos,
		Size = panelSize,
		BgColor = props.color or Color3.new(0, 0, 0)
	})
	local secondPanel = framework.Frame(container, {
		Position = secondEndPos,
		Size = panelSize,
		BgColor = props.color or Color3.new(0, 0, 0)
	})

	local tweenInfo = TweenInfo.new(props.duration or 1, Enum.EasingStyle.Quad, Enum.EasingDirection.InOut)
	local introFirst = TweenService:Create(firstPanel, tweenInfo, {Position = propsFirstPos})
	local introSecond = TweenService:Create(secondPanel, tweenInfo, {Position = propsSecondPos})

	introFirst:Play()
	introSecond:Play()

	introSecond.Completed:Once(function()
        task.wait(props.waiting_time or 0)
		local outroFirst = TweenService:Create(firstPanel, tweenInfo, {Position = firstEndPos})
		local outroSecond = TweenService:Create(secondPanel, tweenInfo, {Position = secondEndPos})
		outroFirst:Play()
		outroSecond:Play()
		outroSecond.Completed:Once(function()
			container:Destroy()
		end)
	end)

	return container
end
